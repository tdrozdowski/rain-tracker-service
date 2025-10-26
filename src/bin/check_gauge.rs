use clap::Parser;
use sqlx::postgres::PgPoolOptions;

#[derive(Parser)]
#[command(name = "check-gauge")]
#[command(about = "Check gauge data in database", long_about = None)]
struct Cli {
    /// Gauge ID to check
    gauge_id: String,

    /// Database connection string
    #[arg(long, env)]
    database_url: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();
    let cli = Cli::parse();

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&cli.database_url)
        .await?;

    println!("Checking gauge {}...\n", cli.gauge_id);

    // Check by data source
    let sources = sqlx::query!(
        r#"
        SELECT data_source, COUNT(*) as "count!"
        FROM rain_readings
        WHERE station_id = $1
        GROUP BY data_source
        ORDER BY data_source
        "#,
        cli.gauge_id
    )
    .fetch_all(&pool)
    .await?;

    println!("Data sources for gauge {}:", cli.gauge_id);
    for row in &sources {
        println!("  {}: {} readings", row.data_source, row.count);
    }

    // Check total count
    let total = sqlx::query!(
        r#"
        SELECT COUNT(*) as "count!"
        FROM rain_readings
        WHERE station_id = $1
        "#,
        cli.gauge_id
    )
    .fetch_one(&pool)
    .await?;

    println!("\nTotal readings for {}: {}", cli.gauge_id, total.count);

    // Check water year 2017 specifically
    let wy2017 = sqlx::query!(
        r#"
        SELECT COUNT(*) as "count!"
        FROM rain_readings
        WHERE station_id = $1
        AND reading_datetime >= '2016-10-01'
        AND reading_datetime < '2017-10-01'
        "#,
        cli.gauge_id
    )
    .fetch_one(&pool)
    .await?;

    println!(
        "Water year 2017 (Oct 2016 - Sep 2017): {} readings",
        wy2017.count
    );

    // Check if gauge exists at all in recent import
    let recent = sqlx::query!(
        r#"
        SELECT reading_datetime, incremental_inches, data_source
        FROM rain_readings
        WHERE station_id = $1
        ORDER BY reading_datetime DESC
        LIMIT 10
        "#,
        cli.gauge_id
    )
    .fetch_all(&pool)
    .await?;

    println!("\nRecent readings for {}:", cli.gauge_id);
    for row in recent {
        println!(
            "  {}: {:.2}\" from {}",
            row.reading_datetime, row.incremental_inches, row.data_source
        );
    }

    // Check what gauges were imported from pdf_WY_2017
    let pdf_gauges = sqlx::query!(
        r#"
        SELECT DISTINCT station_id
        FROM rain_readings
        WHERE data_source = 'pdf_WY_2017'
        AND station_id LIKE '59%'
        ORDER BY station_id
        "#
    )
    .fetch_all(&pool)
    .await?;

    println!("\nGauges starting with '59' from pdf_WY_2017:");
    for row in pdf_gauges {
        println!("  {}", row.station_id);
    }

    // Check all 5-digit gauges from 2017
    let five_digit = sqlx::query!(
        r#"
        SELECT DISTINCT station_id
        FROM rain_readings
        WHERE data_source = 'pdf_WY_2017'
        AND LENGTH(station_id) = 5
        ORDER BY station_id
        "#
    )
    .fetch_all(&pool)
    .await?;

    println!(
        "\nAll 5-digit gauges from pdf_WY_2017 ({} total):",
        five_digit.len()
    );
    for (i, row) in five_digit.iter().enumerate() {
        if i % 10 == 0 && i > 0 {
            println!();
        }
        print!("  {}", row.station_id);
    }
    println!("\n");

    // Check if gauge 59700 exists in any gauge group in a downloaded PDF
    println!("Checking if 59700 might be missing from the source PDFs...");
    println!(
        "(Gauge 59700 has {} readings from excel_WY_2023, so it exists in newer data)",
        sources
            .iter()
            .find(|s| s.data_source == "excel_WY_2023")
            .map(|s| s.count)
            .unwrap_or(0)
    );

    Ok(())
}
