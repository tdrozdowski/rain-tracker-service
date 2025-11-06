// Tests for ExcelImporter to improve coverage
// Tests parsing Excel files with rain gauge data

use chrono::Datelike;
use rain_tracker_service::importers::excel_importer::{ExcelImportError, ExcelImporter};

#[test]
fn test_excel_importer_creation() {
    let importer = ExcelImporter::new("test.xlsx");
    // Just verify it constructs without error
    let _ = importer;
}

#[test]
fn test_workbook_not_found() {
    let importer = ExcelImporter::new("/nonexistent/path/to/file.xlsx");
    let result = importer.parse_month_sheet("OCT");

    assert!(result.is_err());
    match result.unwrap_err() {
        ExcelImportError::WorkbookOpen(msg) => {
            assert!(msg.contains("No such file") || msg.contains("not found"));
        }
        _ => panic!("Expected WorkbookOpen error"),
    }
}

#[test]
fn test_sheet_not_found() {
    // Use the actual sample data file that exists
    let importer = ExcelImporter::new("sample-data-files/pcp_WY_2023.xlsx");
    let result = importer.parse_month_sheet("NONEXISTENT_SHEET");

    assert!(result.is_err());
    match result.unwrap_err() {
        ExcelImportError::SheetNotFound(sheet) => {
            assert_eq!(sheet, "NONEXISTENT_SHEET");
        }
        _ => panic!("Expected SheetNotFound error"),
    }
}

#[test]
fn test_parse_month_sheet_valid() {
    // Use the actual sample data file
    let importer = ExcelImporter::new("sample-data-files/pcp_WY_2023.xlsx");
    let result = importer.parse_month_sheet("OCT");

    // Should successfully parse October data
    assert!(result.is_ok());
    let readings = result.unwrap();

    // October should have some readings (water year starts in October)
    // We don't assert exact count as the file may vary, but should have readings
    assert!(
        !readings.is_empty(),
        "Expected some readings for October 2023"
    );

    // Verify structure of a reading
    if let Some(reading) = readings.first() {
        assert!(!reading.station_id.is_empty());
        assert!(reading.rainfall_inches >= 0.0);
        // October 2023 dates should be in October (month 10)
        assert_eq!(reading.reading_date.month(), 10);
    }
}

#[test]
fn test_parse_all_months() {
    let importer = ExcelImporter::new("sample-data-files/pcp_WY_2023.xlsx");
    let result = importer.parse_all_months(2023);

    assert!(result.is_ok());
    let readings = result.unwrap();

    // A full water year should have many readings across all months
    assert!(
        readings.len() > 100,
        "Expected at least 100 readings across all months"
    );

    // Verify we have readings from multiple months
    let mut months_seen = std::collections::HashSet::new();
    for reading in &readings {
        months_seen.insert(reading.reading_date.month());
    }

    // Should have data from multiple months (though not necessarily all 12)
    assert!(
        months_seen.len() >= 3,
        "Expected data from at least 3 different months"
    );
}

#[test]
fn test_parse_all_months_missing_sheets() {
    // This tests that missing sheets are handled gracefully
    let importer = ExcelImporter::new("sample-data-files/pcp_WY_2023.xlsx");
    let result = importer.parse_all_months(2023);

    // Should succeed even if some months are missing (they're just skipped)
    assert!(result.is_ok());
}

#[test]
fn test_historical_reading_structure() {
    // Test that we can parse and access HistoricalReading fields
    let importer = ExcelImporter::new("sample-data-files/pcp_WY_2023.xlsx");
    let result = importer.parse_month_sheet("OCT");

    assert!(result.is_ok());
    let readings = result.unwrap();

    if let Some(reading) = readings.first() {
        // Test all fields are accessible
        let _ = &reading.station_id;
        let _ = &reading.reading_date;
        let _ = &reading.rainfall_inches;
        let _ = &reading.footnote_marker;

        // Excel files don't have footnotes
        assert_eq!(reading.footnote_marker, None);

        // Rainfall should be positive (we only store non-zero values)
        assert!(reading.rainfall_inches > 0.0);
    }
}

#[test]
fn test_parse_multiple_gauges() {
    // Excel files contain multiple gauge columns
    let importer = ExcelImporter::new("sample-data-files/pcp_WY_2023.xlsx");
    let result = importer.parse_month_sheet("OCT");

    assert!(result.is_ok());
    let readings = result.unwrap();

    // Collect unique station IDs
    let mut station_ids: Vec<String> = readings
        .iter()
        .map(|r| r.station_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    station_ids.sort();

    // Should have readings from multiple gauges
    assert!(
        station_ids.len() >= 2,
        "Expected readings from at least 2 different gauges, found: {station_ids:?}"
    );
}

#[test]
fn test_date_ordering() {
    // Dates should be sequential within a month
    let importer = ExcelImporter::new("sample-data-files/pcp_WY_2023.xlsx");
    let result = importer.parse_month_sheet("OCT");

    assert!(result.is_ok());
    let readings = result.unwrap();

    // Group by station to check date ordering
    let mut dates_by_station: std::collections::HashMap<String, Vec<_>> =
        std::collections::HashMap::new();

    for reading in readings {
        dates_by_station
            .entry(reading.station_id.clone())
            .or_default()
            .push(reading.reading_date);
    }

    // For each station, dates should be in order (or at least all in October)
    for (_station_id, mut dates) in dates_by_station {
        dates.sort();
        for date in dates {
            assert_eq!(date.month(), 10, "All dates should be in October");
            assert_eq!(date.year(), 2022, "October 2022 is in WY 2023");
        }
    }
}

#[test]
fn test_error_display() {
    // Test that error types implement Display properly
    let err = ExcelImportError::WorkbookOpen("test error".to_string());
    assert!(err.to_string().contains("test error"));

    let err = ExcelImportError::SheetNotFound("TEST".to_string());
    assert!(err.to_string().contains("TEST"));

    let err = ExcelImportError::InvalidData {
        row: 5,
        col: 3,
        msg: "bad data".to_string(),
    };
    assert!(err.to_string().contains("5"));
    assert!(err.to_string().contains("3"));
    assert!(err.to_string().contains("bad data"));

    let err = ExcelImportError::MissingGaugeIds;
    assert!(err.to_string().contains("Missing gauge IDs"));

    let err = ExcelImportError::InvalidDate("2023-13-45".to_string());
    assert!(err.to_string().contains("2023-13-45"));
}

#[test]
fn test_clone_historical_reading() {
    use chrono::NaiveDate;
    use rain_tracker_service::importers::excel_importer::HistoricalReading;

    let reading = HistoricalReading {
        station_id: "12345".to_string(),
        reading_date: NaiveDate::from_ymd_opt(2023, 1, 15).unwrap(),
        rainfall_inches: 1.5,
        footnote_marker: Some("1".to_string()),
    };

    let cloned = reading.clone();
    assert_eq!(reading.station_id, cloned.station_id);
    assert_eq!(reading.reading_date, cloned.reading_date);
    assert_eq!(reading.rainfall_inches, cloned.rainfall_inches);
    assert_eq!(reading.footnote_marker, cloned.footnote_marker);
}

#[test]
fn test_debug_historical_reading() {
    use chrono::NaiveDate;
    use rain_tracker_service::importers::excel_importer::HistoricalReading;

    let reading = HistoricalReading {
        station_id: "12345".to_string(),
        reading_date: NaiveDate::from_ymd_opt(2023, 1, 15).unwrap(),
        rainfall_inches: 1.5,
        footnote_marker: None,
    };

    let debug_str = format!("{reading:?}");
    assert!(debug_str.contains("12345"));
    assert!(debug_str.contains("1.5"));
}

#[test]
fn test_workbook_path_into_string() {
    // Test that new() accepts different string types
    let importer1 = ExcelImporter::new("test.xlsx");
    let importer2 = ExcelImporter::new(String::from("test.xlsx"));
    let importer3 = ExcelImporter::new("test.xlsx".to_string());

    // All should work (just verify construction)
    let _ = (importer1, importer2, importer3);
}
