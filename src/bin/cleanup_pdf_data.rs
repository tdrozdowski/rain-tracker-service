use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    println!("Connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    println!("Deleting old PDF import data (data_source = 'pdf_1119')...");
    let result = sqlx::query!(r#"DELETE FROM rain_readings WHERE data_source = 'pdf_1119'"#)
        .execute(&pool)
        .await?;

    println!("✓ Deleted {} rows", result.rows_affected());

    Ok(())
}
