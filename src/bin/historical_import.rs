use chrono::{Datelike, NaiveDate};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use rain_tracker_service::db::MonthlyRainfallRepository;
use rain_tracker_service::importers::{ExcelImporter, HistoricalReading, PdfImporter};
use sqlx::postgres::PgPoolOptions;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;
use tracing::{error, info};

#[derive(Parser)]
#[command(name = "historical-import")]
#[command(about = "Import historical rain gauge data from MCFCD Excel/PDF files", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Database connection string
    #[arg(long, env)]
    database_url: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Import a single water year from an Excel file
    Excel {
        /// Path to the Excel file (e.g., pcp_WY_2023.xlsx)
        #[arg(short, long)]
        file: PathBuf,

        /// Water year (e.g., 2023 for Oct 2022 - Sep 2023)
        #[arg(short, long)]
        water_year: i32,

        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Import from a PDF file (single month)
    Pdf {
        /// Path to the PDF file (e.g., pcp1119.pdf)
        #[arg(short, long)]
        file: PathBuf,

        /// Month (1-12)
        #[arg(short, long)]
        month: u32,

        /// Year (e.g., 2019)
        #[arg(long)]
        year: i32,

        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },
}

/// Reading with calculated cumulative value
#[derive(Debug, Clone)]
struct ReadingWithCumulative {
    station_id: String,
    reading_date: NaiveDate,
    incremental_inches: f64,
    cumulative_inches: f64,
    footnote_marker: Option<String>,
}

/// Calculate cumulative rainfall values for each station
/// Cumulative is the running total from the start of the water year (Oct 1)
fn calculate_cumulative_values(
    readings: Vec<HistoricalReading>,
    water_year: i32,
) -> Vec<ReadingWithCumulative> {
    // Group readings by station_id
    let mut by_station: HashMap<String, Vec<HistoricalReading>> = HashMap::new();
    for reading in readings {
        by_station
            .entry(reading.station_id.clone())
            .or_default()
            .push(reading);
    }

    let mut result = Vec::new();

    // Process each station independently
    for (station_id, mut station_readings) in by_station {
        // Sort by date (chronological order)
        station_readings.sort_by_key(|r| r.reading_date);

        // Calculate cumulative totals
        let mut cumulative = 0.0;
        let water_year_start = NaiveDate::from_ymd_opt(water_year - 1, 10, 1).unwrap();

        for reading in station_readings {
            // Reset cumulative if we've crossed into a new water year
            if reading.reading_date < water_year_start {
                // This shouldn't happen if we're importing a single water year
                cumulative = 0.0;
            }

            cumulative += reading.rainfall_inches;

            result.push(ReadingWithCumulative {
                station_id: station_id.clone(),
                reading_date: reading.reading_date,
                incremental_inches: reading.rainfall_inches,
                cumulative_inches: cumulative,
                footnote_marker: reading.footnote_marker.clone(),
            });
        }
    }

    result
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    // Connect to database
    info!("Connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&cli.database_url)
        .await?;

    info!("Running migrations...");
    sqlx::migrate!("./migrations").run(&pool).await?;

    match cli.command {
        Commands::Excel {
            file,
            water_year,
            yes,
        } => {
            import_excel(&pool, file, water_year, yes).await?;
        }
        Commands::Pdf {
            file,
            month,
            year,
            yes,
        } => {
            import_pdf(&pool, file, year, month, yes).await?;
        }
    }

    info!("Import completed successfully!");
    Ok(())
}

async fn import_excel(
    pool: &sqlx::PgPool,
    file: PathBuf,
    water_year: i32,
    skip_confirmation: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();

    info!("Importing Excel file: {file:?}");
    info!("Water year: {water_year}");

    // Verify file exists
    if !file.exists() {
        error!("File not found: {file:?}");
        return Err(format!("File not found: {file:?}").into());
    }

    // Confirmation prompt
    if !skip_confirmation {
        println!("\n⚠️  This will import historical data into the database.");
        println!("File: {file:?}");
        println!(
            "Water year: {water_year} (Oct {} - Sep {water_year})",
            water_year - 1
        );
        println!("\nContinue? [y/N]: ");

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Import cancelled.");
            return Ok(());
        }
    }

    // Parse Excel file (blocking operation)
    let parse_start = Instant::now();
    let file_str = file.to_string_lossy().to_string();
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    pb.set_message(format!("Parsing Excel file for water year {water_year}..."));

    let readings = tokio::task::spawn_blocking(move || {
        let importer = ExcelImporter::new(&file_str);
        importer.parse_all_months(water_year)
    })
    .await??;

    let readings_len = readings.len();
    let parse_duration = parse_start.elapsed();
    pb.finish_with_message(format!("✓ Parsed {readings_len} readings"));

    // Calculate cumulative values for each station
    info!("Calculating cumulative rainfall values...");
    let calc_start = Instant::now();
    let readings_with_cumulative = calculate_cumulative_values(readings, water_year);
    let calc_duration = calc_start.elapsed();

    // Insert readings into database
    let insert_start = Instant::now();
    info!("Inserting {readings_len} readings into database...");
    let pb = ProgressBar::new(readings_with_cumulative.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("##-"),
    );

    let data_source = format!("excel_WY_{water_year}");
    let mut inserted = 0;
    let mut duplicates = 0;

    // Track which (station_id, year, month) combinations we inserted data for
    let mut months_to_recalculate: HashSet<(String, i32, u32)> = HashSet::new();

    for reading in readings_with_cumulative {
        // Build import_metadata JSON if we have a footnote
        let import_metadata = reading.footnote_marker.as_ref().map(|marker| {
            serde_json::json!({
                "footnote_marker": marker
            })
        });

        let result = sqlx::query!(
            r#"
            INSERT INTO rain_readings (station_id, reading_datetime, cumulative_inches, incremental_inches, data_source, import_metadata)
            VALUES ($1, $2::date, $3, $4, $5, $6)
            ON CONFLICT (reading_datetime, station_id) DO NOTHING
            "#,
            reading.station_id,
            reading.reading_date,
            reading.cumulative_inches,
            reading.incremental_inches,
            data_source,
            import_metadata as _
        )
        .execute(pool)
        .await?;

        if result.rows_affected() > 0 {
            inserted += 1;
            // Track this month for recalculation
            let year = reading.reading_date.year();
            let month = reading.reading_date.month();
            months_to_recalculate.insert((reading.station_id.clone(), year, month));
        } else {
            duplicates += 1;
        }

        pb.inc(1);
    }

    let insert_duration = insert_start.elapsed();
    pb.finish_with_message(format!(
        "✓ Inserted {inserted} new readings, {duplicates} duplicates skipped"
    ));

    info!("Import summary: {inserted} inserted, {duplicates} duplicates");

    // Recalculate monthly summaries for affected months
    let months_count = months_to_recalculate.len();
    let recalc_duration = if !months_to_recalculate.is_empty() {
        let recalc_start = Instant::now();
        info!(
            "Recalculating monthly summaries for {} station-months...",
            months_count
        );

        let pb = ProgressBar::new(months_count as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} Recalculating...")
                .unwrap()
                .progress_chars("##-"),
        );

        let monthly_repo = MonthlyRainfallRepository::new(pool.clone());

        for (station_id, year, month) in months_to_recalculate {
            monthly_repo
                .recalculate_monthly_summary(&station_id, year, month as i32)
                .await?;
            pb.inc(1);
        }

        let duration = recalc_start.elapsed();
        pb.finish_with_message("✓ Monthly summaries recalculated");
        info!("Monthly summary recalculation complete");
        duration
    } else {
        std::time::Duration::from_secs(0)
    };

    let total_duration = start_time.elapsed();

    // Print performance summary
    println!("\n{}", "=".repeat(60));
    println!("Import Summary");
    println!("{}", "=".repeat(60));
    println!("Water Year:         {water_year}");
    println!("Total Readings:     {readings_len}");
    println!("Inserted:           {inserted}");
    println!("Duplicates:         {duplicates}");
    println!("Station-Months:     {months_count}");
    println!("{}", "-".repeat(60));
    println!("Parse Time:         {:.2}s", parse_duration.as_secs_f64());
    println!("Calculation Time:   {:.2}s", calc_duration.as_secs_f64());
    println!("Insert Time:        {:.2}s", insert_duration.as_secs_f64());
    println!("Recalc Time:        {:.2}s", recalc_duration.as_secs_f64());
    println!("{}", "-".repeat(60));
    println!("Total Time:         {:.2}s", total_duration.as_secs_f64());
    println!("{}", "=".repeat(60));

    if inserted > 0 {
        let rate = inserted as f64 / insert_duration.as_secs_f64();
        println!("Insert Rate:        {rate:.0} readings/sec");
    }

    println!();

    Ok(())
}

async fn import_pdf(
    pool: &sqlx::PgPool,
    file: PathBuf,
    year: i32,
    month: u32,
    skip_confirmation: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();

    info!("Importing PDF file: {file:?}");
    info!("Month/Year: {month}/{year}");

    // Verify file exists
    if !file.exists() {
        error!("File not found: {file:?}");
        return Err(format!("File not found: {file:?}").into());
    }

    // Confirmation prompt
    if !skip_confirmation {
        println!("\n⚠️  This will import historical data into the database.");
        println!("File: {file:?}");
        println!("Month/Year: {month:02}/{year}");
        println!("\nContinue? [y/N]: ");

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Import cancelled.");
            return Ok(());
        }
    }

    // Parse PDF file (blocking operation)
    let parse_start = Instant::now();
    let file_str = file.to_string_lossy().to_string();
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    pb.set_message(format!("Parsing PDF file for {month:02}/{year}..."));

    let readings = tokio::task::spawn_blocking(move || {
        let importer = PdfImporter::new(&file_str);
        importer.parse_all_pages(year, month)
    })
    .await??;

    let readings_len = readings.len();
    let parse_duration = parse_start.elapsed();
    pb.finish_with_message(format!("✓ Parsed {readings_len} readings"));

    // Calculate cumulative values for each station
    // Note: PDF data is per-month, so we'll use month/year for the cumulative calculation
    // This is different from water year Excel imports
    info!("Calculating cumulative rainfall values...");
    let calc_start = Instant::now();
    let readings_with_cumulative = calculate_cumulative_values_monthly(readings, year, month);
    let calc_duration = calc_start.elapsed();

    // Insert readings into database
    let insert_start = Instant::now();
    info!("Inserting {readings_len} readings into database...");
    let pb = ProgressBar::new(readings_with_cumulative.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("##-"),
    );

    let data_source = format!("pdf_{month:02}{}", year % 100);
    let mut inserted = 0;
    let mut duplicates = 0;

    // Track which (station_id, year, month) combinations we inserted data for
    let mut months_to_recalculate: HashSet<(String, i32, u32)> = HashSet::new();

    for reading in readings_with_cumulative {
        // Build import_metadata JSON if we have a footnote
        let import_metadata = reading.footnote_marker.as_ref().map(|marker| {
            serde_json::json!({
                "footnote_marker": marker
            })
        });

        let result = sqlx::query!(
            r#"
            INSERT INTO rain_readings (station_id, reading_datetime, cumulative_inches, incremental_inches, data_source, import_metadata)
            VALUES ($1, $2::date, $3, $4, $5, $6)
            ON CONFLICT (reading_datetime, station_id) DO NOTHING
            "#,
            reading.station_id,
            reading.reading_date,
            reading.cumulative_inches,
            reading.incremental_inches,
            data_source,
            import_metadata as _
        )
        .execute(pool)
        .await?;

        if result.rows_affected() > 0 {
            inserted += 1;
            // Track this month for recalculation
            let year = reading.reading_date.year();
            let month = reading.reading_date.month();
            months_to_recalculate.insert((reading.station_id.clone(), year, month));
        } else {
            duplicates += 1;
        }

        pb.inc(1);
    }

    let insert_duration = insert_start.elapsed();
    pb.finish_with_message(format!(
        "✓ Inserted {inserted} new readings, {duplicates} duplicates skipped"
    ));

    info!("Import summary: {inserted} inserted, {duplicates} duplicates");

    // Recalculate monthly summaries for affected months
    let months_count = months_to_recalculate.len();
    let recalc_duration = if !months_to_recalculate.is_empty() {
        let recalc_start = Instant::now();
        info!(
            "Recalculating monthly summaries for {} station-months...",
            months_count
        );

        let pb = ProgressBar::new(months_count as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} Recalculating...")
                .unwrap()
                .progress_chars("##-"),
        );

        let monthly_repo = MonthlyRainfallRepository::new(pool.clone());

        for (station_id, year, month) in months_to_recalculate {
            monthly_repo
                .recalculate_monthly_summary(&station_id, year, month as i32)
                .await?;
            pb.inc(1);
        }

        let duration = recalc_start.elapsed();
        pb.finish_with_message("✓ Monthly summaries recalculated");
        info!("Monthly summary recalculation complete");
        duration
    } else {
        std::time::Duration::from_secs(0)
    };

    let total_duration = start_time.elapsed();

    // Print performance summary
    println!("\n{}", "=".repeat(60));
    println!("Import Summary");
    println!("{}", "=".repeat(60));
    println!("Month/Year:         {month:02}/{year}");
    println!("Total Readings:     {readings_len}");
    println!("Inserted:           {inserted}");
    println!("Duplicates:         {duplicates}");
    println!("Station-Months:     {months_count}");
    println!("{}", "-".repeat(60));
    println!("Parse Time:         {:.2}s", parse_duration.as_secs_f64());
    println!("Calculation Time:   {:.2}s", calc_duration.as_secs_f64());
    println!("Insert Time:        {:.2}s", insert_duration.as_secs_f64());
    println!("Recalc Time:        {:.2}s", recalc_duration.as_secs_f64());
    println!("{}", "-".repeat(60));
    println!("Total Time:         {:.2}s", total_duration.as_secs_f64());
    println!("{}", "=".repeat(60));

    if inserted > 0 {
        let rate = inserted as f64 / insert_duration.as_secs_f64();
        println!("Insert Rate:        {rate:.0} readings/sec");
    }

    println!();

    Ok(())
}

/// Calculate cumulative values for a single month
/// Unlike water year calculations, this resets at the start of each month
fn calculate_cumulative_values_monthly(
    readings: Vec<HistoricalReading>,
    year: i32,
    month: u32,
) -> Vec<ReadingWithCumulative> {
    // Group readings by station_id
    let mut by_station: HashMap<String, Vec<HistoricalReading>> = HashMap::new();
    for reading in readings {
        by_station
            .entry(reading.station_id.clone())
            .or_default()
            .push(reading);
    }

    let mut result = Vec::new();

    // Process each station independently
    for (station_id, mut station_readings) in by_station {
        // Sort by date (chronological order)
        station_readings.sort_by_key(|r| r.reading_date);

        // Calculate cumulative totals within the month
        let mut cumulative = 0.0;
        let month_start = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
        let month_end = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap()
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap()
        };

        for reading in station_readings {
            // Only process readings within the expected month
            if reading.reading_date >= month_start && reading.reading_date < month_end {
                cumulative += reading.rainfall_inches;

                result.push(ReadingWithCumulative {
                    station_id: station_id.clone(),
                    reading_date: reading.reading_date,
                    incremental_inches: reading.rainfall_inches,
                    cumulative_inches: cumulative,
                    footnote_marker: reading.footnote_marker.clone(),
                });
            }
        }
    }

    result
}
