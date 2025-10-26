use chrono::{Datelike, NaiveDate};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use rain_tracker_service::db::MonthlyRainfallRepository;
use rain_tracker_service::importers::{
    ExcelImporter, HistoricalReading, McfcdDownloader, PdfImporter,
};
use sqlx::postgres::PgPoolOptions;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;
use tracing::{error, info};

#[derive(Parser)]
#[command(name = "historical-import")]
#[command(about = "Import historical rain gauge data from MCFCD Excel/PDF files", long_about = None)]
struct Cli {
    /// Database connection string
    #[arg(long, env)]
    database_url: String,

    /// Import mode: 'single' (download one year), 'bulk' (download range), 'excel' (local file), 'pdf' (local file)
    #[arg(long)]
    mode: String,

    /// Water year (e.g., 2023 for Oct 2022 - Sep 2023)
    #[arg(long)]
    water_year: Option<i32>,

    /// Start year for bulk mode
    #[arg(long)]
    start_year: Option<i32>,

    /// End year for bulk mode
    #[arg(long)]
    end_year: Option<i32>,

    /// Path to local Excel or PDF file (for 'excel' or 'pdf' modes)
    #[arg(long)]
    file: Option<PathBuf>,

    /// Month (1-12, for PDF mode only)
    #[arg(long)]
    month: Option<u32>,

    /// Year (for PDF mode only)
    #[arg(long)]
    year: Option<i32>,

    /// Skip confirmation prompt
    #[arg(short = 'y', long)]
    yes: bool,

    /// Keep downloaded files instead of deleting them
    #[arg(long)]
    keep_files: bool,

    /// Directory to save downloaded files (default: /tmp)
    #[arg(long, default_value = "/tmp")]
    output_dir: String,
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
    // Load .env file if it exists (ignore errors if not found)
    let _ = dotenvy::dotenv();

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

    match cli.mode.as_str() {
        "single" => {
            let water_year = cli
                .water_year
                .ok_or("--water-year is required for single mode")?;
            load_water_year(&pool, water_year, cli.yes, cli.keep_files, &cli.output_dir).await?;
        }
        "bulk" => {
            let start_year = cli
                .start_year
                .ok_or("--start-year is required for bulk mode")?;
            let end_year = cli.end_year.ok_or("--end-year is required for bulk mode")?;
            load_bulk_years(
                &pool,
                start_year,
                end_year,
                cli.yes,
                cli.keep_files,
                &cli.output_dir,
            )
            .await?;
        }
        "excel" => {
            let file = cli.file.ok_or("--file is required for excel mode")?;
            let water_year = cli
                .water_year
                .ok_or("--water-year is required for excel mode")?;
            import_excel(&pool, file, water_year, cli.yes).await?;
        }
        "pdf" => {
            let file = cli.file.ok_or("--file is required for pdf mode")?;
            let month = cli.month.ok_or("--month is required for pdf mode")?;
            let year = cli.year.ok_or("--year is required for pdf mode")?;
            import_pdf(&pool, file, year, month, cli.yes).await?;
        }
        _ => {
            return Err(format!(
                "Invalid mode '{}'. Valid modes: single, bulk, excel, pdf",
                cli.mode
            )
            .into());
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

/// Smart loader: Downloads and imports a water year from MCFCD
/// Automatically chooses Excel (2022+) or PDF (pre-2022) format
async fn load_water_year(
    pool: &sqlx::PgPool,
    water_year: i32,
    skip_confirmation: bool,
    keep_files: bool,
    output_dir: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();

    // Determine format based on water year
    // Excel files available for WY 2022 and later
    // PDF files for earlier years
    const EXCEL_CUTOFF_YEAR: i32 = 2022;

    let use_excel = water_year >= EXCEL_CUTOFF_YEAR;
    let format = if use_excel { "Excel" } else { "PDF" };

    info!("Loading water year {} using {} format", water_year, format);

    // Confirmation prompt
    if !skip_confirmation {
        println!("\n⚠️  This will download and import historical data from MCFCD.");
        println!(
            "Water year: {} (Oct {} - Sep {})",
            water_year,
            water_year - 1,
            water_year
        );
        println!("Format: {format}");
        if use_excel {
            println!("Files: 1 Excel file (pcp_WY_{water_year}.xlsx)");
        } else {
            println!(
                "Files: 12 monthly PDFs (pcp{:02}{:02}.pdf through pcp{:02}{:02}.pdf)",
                10,
                (water_year - 1) % 100,
                9,
                water_year % 100
            );
        }
        println!("\nContinue? [y/N]: ");

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Import cancelled.");
            return Ok(());
        }
    }

    // Create output directory if it doesn't exist
    std::fs::create_dir_all(output_dir)?;

    let downloader = McfcdDownloader::new();

    if use_excel {
        // Download and import Excel file
        info!("Downloading Excel file for water year {}...", water_year);
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        pb.set_message(format!("Downloading pcp_WY_{water_year}.xlsx..."));

        let excel_bytes = downloader.download_excel(water_year).await?;
        pb.finish_with_message(format!(
            "✓ Downloaded Excel file ({} KB)",
            excel_bytes.len() / 1024
        ));

        // Parse Excel from memory
        info!("Parsing Excel file...");
        let parse_start = Instant::now();
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );
        pb.set_message(format!("Parsing Excel file for water year {water_year}..."));

        // Write to file for Excel parser
        let temp_file = format!("{output_dir}/pcp_WY_{water_year}.xlsx");
        std::fs::write(&temp_file, &excel_bytes)?;
        if keep_files {
            info!("Saved Excel file to: {}", temp_file);
        }

        let should_delete = !keep_files;
        let readings = tokio::task::spawn_blocking(move || {
            let importer = ExcelImporter::new(&temp_file);
            let result = importer.parse_all_months(water_year);
            // Clean up temp file if not keeping
            if should_delete {
                let _ = std::fs::remove_file(&temp_file);
            }
            result
        })
        .await??;

        let readings_len = readings.len();
        let parse_duration = parse_start.elapsed();
        pb.finish_with_message(format!("✓ Parsed {readings_len} readings"));

        // Calculate and insert (reuse existing logic)
        info!("Calculating cumulative rainfall values...");
        let calc_start = Instant::now();
        let readings_with_cumulative = calculate_cumulative_values(readings, water_year);
        let calc_duration = calc_start.elapsed();

        let (inserted, duplicates, months_count, insert_duration, recalc_duration) =
            insert_readings_batch(
                pool,
                readings_with_cumulative.clone(),
                format!("excel_WY_{water_year}"),
            )
            .await?;

        // Print gauge coverage summary
        print_gauge_summary(&readings_with_cumulative, water_year);

        let total_duration = start_time.elapsed();
        print_summary(
            water_year,
            format,
            readings_len,
            inserted,
            duplicates,
            months_count,
            parse_duration,
            calc_duration,
            insert_duration,
            recalc_duration,
            total_duration,
        );
    } else {
        // Download and import 12 monthly PDFs
        info!(
            "Downloading 12 monthly PDFs for water year {}...",
            water_year
        );
        let pb = ProgressBar::new(12);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("##-"),
        );

        let pdfs = downloader.download_water_year_pdfs(water_year).await?;
        pb.finish_with_message(format!("✓ Downloaded {} PDF files", pdfs.len()));

        let mut all_readings = Vec::new();
        let mut total_parse_duration = std::time::Duration::from_secs(0);

        info!("Parsing {} PDF files...", pdfs.len());
        let pb = ProgressBar::new(pdfs.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} Parsing...")
                .unwrap()
                .progress_chars("##-"),
        );

        for (month, year, pdf_bytes) in pdfs {
            let parse_start = Instant::now();

            // Write to file for PDF parser (pdf-extract requires file path)
            let temp_file = format!("{}/pcp{:02}{:02}.pdf", output_dir, month, year % 100);
            std::fs::write(&temp_file, &pdf_bytes)?;

            let temp_file_clone = temp_file.clone();
            let readings = tokio::task::spawn_blocking(move || {
                let importer = PdfImporter::new(&temp_file_clone);
                importer.parse_all_pages(year, month)
            })
            .await??;

            // Clean up temp file if not keeping
            if !keep_files {
                std::fs::remove_file(&temp_file)?;
            }

            total_parse_duration += parse_start.elapsed();
            all_readings.extend(readings);
            pb.inc(1);
        }

        let readings_len = all_readings.len();
        pb.finish_with_message(format!("✓ Parsed {readings_len} total readings"));

        if keep_files {
            info!(
                "Saved 12 PDF files to: {}/pcp{{MMYY}}.pdf (Oct {} - Sep {})",
                output_dir,
                water_year - 1,
                water_year
            );
        }

        // Calculate cumulative values
        info!("Calculating cumulative rainfall values...");
        let calc_start = Instant::now();
        let readings_with_cumulative = calculate_cumulative_values(all_readings, water_year);
        let calc_duration = calc_start.elapsed();

        let (inserted, duplicates, months_count, insert_duration, recalc_duration) =
            insert_readings_batch(
                pool,
                readings_with_cumulative.clone(),
                format!("pdf_WY_{water_year}"),
            )
            .await?;

        // Print gauge coverage summary
        print_gauge_summary(&readings_with_cumulative, water_year);

        let total_duration = start_time.elapsed();
        print_summary(
            water_year,
            format,
            readings_len,
            inserted,
            duplicates,
            months_count,
            total_parse_duration,
            calc_duration,
            insert_duration,
            recalc_duration,
            total_duration,
        );
    }

    Ok(())
}

/// Load multiple water years in sequence
async fn load_bulk_years(
    pool: &sqlx::PgPool,
    start_year: i32,
    end_year: i32,
    skip_confirmation: bool,
    keep_files: bool,
    output_dir: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if start_year > end_year {
        return Err("start-year must be <= end-year".into());
    }

    let year_count = end_year - start_year + 1;

    if !skip_confirmation {
        println!("\n⚠️  This will download and import {year_count} water years from MCFCD.");
        println!("Range: WY {start_year} through WY {end_year}");
        println!("\nContinue? [y/N]: ");

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Import cancelled.");
            return Ok(());
        }
    }

    info!("Starting bulk import for {} water years", year_count);
    let bulk_start = Instant::now();

    for water_year in start_year..=end_year {
        println!("\n{}", "=".repeat(60));
        println!(
            "Processing Water Year {} ({}/{})",
            water_year,
            water_year - start_year + 1,
            year_count
        );
        println!("{}", "=".repeat(60));

        match load_water_year(pool, water_year, true, keep_files, output_dir).await {
            Ok(_) => {
                info!("✓ Water year {} completed successfully", water_year);
            }
            Err(e) => {
                error!("✗ Water year {} failed: {}", water_year, e);
                println!("\n⚠️  Error importing water year {water_year}: {e}");
                println!("Continue with remaining years? [y/N]: ");

                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    println!("Bulk import cancelled.");
                    return Err(e);
                }
            }
        }
    }

    let bulk_duration = bulk_start.elapsed();
    println!("\n{}", "=".repeat(60));
    println!("Bulk Import Complete");
    println!("{}", "=".repeat(60));
    println!("Total Years:        {year_count}");
    println!("Total Time:         {:.2}s", bulk_duration.as_secs_f64());
    println!(
        "Average per Year:   {:.2}s",
        bulk_duration.as_secs_f64() / year_count as f64
    );
    println!("{}", "=".repeat(60));

    Ok(())
}

/// Insert a batch of readings into the database
/// Returns: (inserted, duplicates, months_count, insert_duration, recalc_duration)
async fn insert_readings_batch(
    pool: &sqlx::PgPool,
    readings: Vec<ReadingWithCumulative>,
    data_source: String,
) -> Result<
    (
        usize,
        usize,
        usize,
        std::time::Duration,
        std::time::Duration,
    ),
    Box<dyn std::error::Error>,
> {
    let insert_start = Instant::now();
    let readings_len = readings.len();

    info!("Inserting {} readings into database...", readings_len);
    let pb = ProgressBar::new(readings_len as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("##-"),
    );

    let mut inserted = 0;
    let mut duplicates = 0;
    let mut months_to_recalculate: HashSet<(String, i32, u32)> = HashSet::new();

    for reading in readings {
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

    info!(
        "Insert summary: {} inserted, {} duplicates",
        inserted, duplicates
    );

    // Recalculate monthly summaries
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

    Ok((
        inserted,
        duplicates,
        months_count,
        insert_duration,
        recalc_duration,
    ))
}

/// Print gauge coverage summary table
fn print_gauge_summary(readings: &[ReadingWithCumulative], water_year: i32) {
    // Group by gauge and month
    let mut gauge_months: BTreeMap<String, HashSet<(i32, u32)>> = BTreeMap::new();

    for reading in readings {
        let year = reading.reading_date.year();
        let month = reading.reading_date.month();
        gauge_months
            .entry(reading.station_id.clone())
            .or_default()
            .insert((year, month));
    }

    if gauge_months.is_empty() {
        return;
    }

    println!("\n{}", "=".repeat(80));
    println!("Gauge Coverage Summary - Water Year {water_year}");
    println!("{}", "=".repeat(80));
    println!("{:<10} {:>8}  Months with Data", "Gauge ID", "Readings");
    println!("{}", "-".repeat(80));

    for (gauge_id, months) in gauge_months.iter() {
        let reading_count = readings
            .iter()
            .filter(|r| r.station_id == *gauge_id)
            .count();

        // Sort months chronologically
        let mut sorted_months: Vec<_> = months.iter().collect();
        sorted_months.sort();

        // Format months as "Oct", "Nov", etc.
        let month_names: Vec<String> = sorted_months
            .iter()
            .map(|(y, m)| {
                let name = match m {
                    10 => "Oct",
                    11 => "Nov",
                    12 => "Dec",
                    1 => "Jan",
                    2 => "Feb",
                    3 => "Mar",
                    4 => "Apr",
                    5 => "May",
                    6 => "Jun",
                    7 => "Jul",
                    8 => "Aug",
                    9 => "Sep",
                    _ => "?",
                };
                // Show year if mixed years (shouldn't happen in single water year import)
                if sorted_months.iter().any(|(year, _)| year != y) {
                    format!("{} {}", name, y % 100)
                } else {
                    name.to_string()
                }
            })
            .collect();

        println!(
            "{:<10} {:>8}  {}",
            gauge_id,
            reading_count,
            month_names.join(", ")
        );
    }

    println!("{}", "=".repeat(80));
    println!("Total gauges: {}", gauge_months.len());
    println!("{}", "=".repeat(80));
}

/// Print import summary
#[allow(clippy::too_many_arguments)]
fn print_summary(
    water_year: i32,
    format: &str,
    total_readings: usize,
    inserted: usize,
    duplicates: usize,
    months_count: usize,
    parse_duration: std::time::Duration,
    calc_duration: std::time::Duration,
    insert_duration: std::time::Duration,
    recalc_duration: std::time::Duration,
    total_duration: std::time::Duration,
) {
    println!("\n{}", "=".repeat(60));
    println!("Import Summary");
    println!("{}", "=".repeat(60));
    println!("Water Year:         {water_year}");
    println!("Format:             {format}");
    println!("Total Readings:     {total_readings}");
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
}
