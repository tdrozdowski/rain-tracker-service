use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use rain_tracker_service::importers::ExcelImporter;
use sqlx::postgres::PgPoolOptions;
use std::path::PathBuf;
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

    /// Import from a PDF file (not yet implemented)
    Pdf {
        /// Path to the PDF file (e.g., pcp1119.pdf)
        #[arg(short, long)]
        file: PathBuf,

        /// Month (1-12)
        #[arg(short, long)]
        month: u32,

        /// Year (e.g., 2019)
        #[arg(short, long)]
        year: i32,
    },
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
        Commands::Pdf { .. } => {
            error!("PDF import is not yet implemented");
            return Err("PDF import not implemented".into());
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
    pb.finish_with_message(format!("✓ Parsed {readings_len} readings"));

    // Insert readings into database
    info!("Inserting {readings_len} readings into database...");
    let pb = ProgressBar::new(readings.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("##-"),
    );

    let data_source = format!("excel_WY_{water_year}");
    let mut inserted = 0;
    let mut duplicates = 0;

    for reading in readings {
        let result = sqlx::query!(
            r#"
            INSERT INTO rain_readings (station_id, reading_datetime, cumulative_inches, incremental_inches, data_source)
            VALUES ($1, $2::date, 0.0, $3, $4)
            ON CONFLICT (reading_datetime, station_id) DO NOTHING
            "#,
            reading.station_id,
            reading.reading_date,
            reading.rainfall_inches,
            data_source
        )
        .execute(pool)
        .await?;

        if result.rows_affected() > 0 {
            inserted += 1;
        } else {
            duplicates += 1;
        }

        pb.inc(1);
    }

    pb.finish_with_message(format!(
        "✓ Inserted {inserted} new readings, {duplicates} duplicates skipped"
    ));

    info!("Import summary: {inserted} inserted, {duplicates} duplicates");

    Ok(())
}
