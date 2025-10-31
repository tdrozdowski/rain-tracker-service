use rain_tracker_service::importers::{HistoricalReading, PdfImporter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let importer = PdfImporter::new("plans/pcp1119.pdf");
    let readings = importer.parse_all_pages(2019, 11)?;

    // Filter for gauges 62000 and 62200
    let gauge_62000: Vec<&HistoricalReading> = readings
        .iter()
        .filter(|r| r.station_id == "62000")
        .collect();

    let gauge_62200: Vec<&HistoricalReading> = readings
        .iter()
        .filter(|r| r.station_id == "62200")
        .collect();

    println!("\n=== Gauge 62000 ===");
    println!("Total readings: {}", gauge_62000.len());
    let mut total_62000 = 0.0;
    for r in gauge_62000 {
        let footnote = r
            .footnote_marker
            .as_ref()
            .map(|m| format!(" ({m})"))
            .unwrap_or_default();
        println!("  {}: {:.2}\"{footnote}", r.reading_date, r.rainfall_inches);
        total_62000 += r.rainfall_inches;
    }
    println!("Total rainfall: {total_62000:.2}\" (expected: 3.78\")");

    println!("\n=== Gauge 62200 ===");
    println!("Total readings: {}", gauge_62200.len());
    let mut total_62200 = 0.0;
    for r in gauge_62200 {
        let footnote = r
            .footnote_marker
            .as_ref()
            .map(|m| format!(" ({m})"))
            .unwrap_or_default();
        println!("  {}: {:.2}\"{footnote}", r.reading_date, r.rainfall_inches);
        total_62200 += r.rainfall_inches;
    }
    println!("Total rainfall: {total_62200:.2}\" (expected: 0.00\")");
    println!("Note: Gauge 62200 was inoperative in November 2019\n");

    Ok(())
}
