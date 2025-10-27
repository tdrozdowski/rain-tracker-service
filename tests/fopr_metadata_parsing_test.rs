/// Integration tests for FOPR Meta_Stats parsing
///
/// These tests parse actual sample FOPR files to validate the metadata extraction logic.
use calamine::{open_workbook_auto, Reader};
use chrono::Datelike;
use rain_tracker_service::fopr::MetaStatsData;

#[test]
fn test_parse_meta_stats_59700() {
    let mut workbook = open_workbook_auto("sample-data-files/59700_FOPR.xlsx")
        .expect("Failed to open 59700_FOPR.xlsx");

    let range = workbook
        .worksheet_range("Meta_Stats")
        .expect("Failed to find Meta_Stats sheet");

    let meta =
        MetaStatsData::from_worksheet_range(&range).expect("Failed to parse Meta_Stats data");

    // Identification
    assert_eq!(meta.station_id, "59700");
    assert_eq!(meta.station_name, "Aztec Park");
    assert_eq!(meta.previous_station_ids, vec!["4695"]);
    assert_eq!(meta.station_type, "Rain");

    // Location
    assert_eq!(meta.latitude, 33.61006);
    assert_eq!(meta.longitude, -111.86545);
    assert_eq!(meta.elevation_ft, Some(1465));
    assert_eq!(meta.city, Some("Scottsdale".to_string()));
    assert_eq!(meta.county, "Maricopa");
    assert_eq!(
        meta.location_description,
        Some("Near Thunderbird & Frank Lloyd Wright".to_string())
    );

    // Operational metadata
    assert_eq!(meta.status, "Active");
    assert!(meta.data_begins_date.is_some());
    let data_begins = meta.data_begins_date.unwrap();
    assert_eq!(data_begins.year(), 1998);
    assert_eq!(data_begins.month(), 2);
    assert_eq!(data_begins.day(), 9); // 35835 = Feb 9, 1998

    assert!(meta.installation_date.is_some());
    let install_date = meta.installation_date.unwrap();
    assert_eq!(install_date.year(), 1998);
    // Installation date calculated from years_since and reference_date
    // Oct 1, 2024 - 26.64 years ≈ Feb 1998
    assert!(install_date.month() >= 1 && install_date.month() <= 3); // Allow Jan-Mar range

    // Climate statistics
    assert_eq!(meta.complete_years_count, Some(26));
    assert!(meta.avg_annual_precipitation_inches.is_some());
    let precip = meta.avg_annual_precipitation_inches.unwrap();
    assert!((precip - 7.4803).abs() < 0.001);

    // Data quality
    assert_eq!(meta.incomplete_months_count, 0); // "None" → 0
    assert_eq!(meta.missing_months_count, 0); // "None" → 0
    assert_eq!(meta.data_quality_remarks, Some("Records Good".to_string()));

    // FOPR metadata JSONB
    let storms_1in = meta.fopr_metadata.get("storms_gt_1in_24h");
    assert!(storms_1in.is_some());
    assert_eq!(storms_1in.unwrap().as_i64(), Some(35));

    let storms_2in = meta.fopr_metadata.get("storms_gt_2in_24h");
    assert!(storms_2in.is_some());
    assert_eq!(storms_2in.unwrap().as_i64(), Some(4));

    let storms_3in = meta.fopr_metadata.get("storms_gt_3in_24h");
    assert!(storms_3in.is_some());
    assert_eq!(storms_3in.unwrap().as_i64(), Some(0));

    // Frequency stats - 15 min
    let freq_15min = meta.fopr_metadata.get("freq_15min_inches");
    assert!(freq_15min.is_some());
    assert!((freq_15min.unwrap().as_f64().unwrap() - 0.91).abs() < 0.001);

    // Frequency stats - 24 hour
    let freq_24hr = meta.fopr_metadata.get("freq_24hr_inches");
    assert!(freq_24hr.is_some());
    assert!((freq_24hr.unwrap().as_f64().unwrap() - 2.64).abs() < 0.001);

    // Verify date conversions in frequency stats
    let freq_15min_date = meta.fopr_metadata.get("freq_15min_date");
    assert!(freq_15min_date.is_some());
    // 38566 should convert to a valid date string
    assert!(freq_15min_date.unwrap().is_string());

    // Verify return periods
    let freq_15min_period = meta.fopr_metadata.get("freq_15min_return_period_yrs");
    assert!(freq_15min_period.is_some());
    assert_eq!(freq_15min_period.unwrap().as_i64(), Some(20));
}

#[test]
fn test_parse_meta_stats_11000() {
    let mut workbook = open_workbook_auto("sample-data-files/11000_FOPR.xlsx")
        .expect("Failed to open 11000_FOPR.xlsx");

    let range = workbook
        .worksheet_range("Meta_Stats")
        .expect("Failed to find Meta_Stats sheet");

    let meta =
        MetaStatsData::from_worksheet_range(&range).expect("Failed to parse Meta_Stats data");

    // Identification
    assert_eq!(meta.station_id, "11000");
    assert_eq!(meta.station_name, "10th St. Wash Basin # 1");
    assert_eq!(meta.previous_station_ids, vec!["4815"]);
    assert_eq!(meta.station_type, "Rain / Stage"); // Different type

    // Location
    assert_eq!(meta.latitude, 33.57969);
    assert_eq!(meta.longitude, -112.05476);
    assert_eq!(meta.elevation_ft, Some(1320)); // No comma in original
    assert_eq!(meta.city, Some("Phoenix".to_string()));
    assert_eq!(meta.county, "Maricopa");
    assert_eq!(
        meta.location_description,
        Some("1/4 mi. SW of Peoria Ave. & Cave Creek Rd.".to_string())
    );

    // Climate statistics
    assert_eq!(meta.complete_years_count, Some(27)); // Different count
    assert!(meta.avg_annual_precipitation_inches.is_some());
    let precip = meta.avg_annual_precipitation_inches.unwrap();
    assert!((precip - 6.389666296296296).abs() < 0.001);

    // Data quality
    assert_eq!(meta.incomplete_months_count, 0);
    assert_eq!(meta.missing_months_count, 0);
    assert_eq!(meta.data_quality_remarks, Some("Records Good".to_string()));

    // FOPR metadata JSONB - different storm counts
    let storms_1in = meta.fopr_metadata.get("storms_gt_1in_24h");
    assert_eq!(storms_1in.unwrap().as_i64(), Some(29));

    let storms_2in = meta.fopr_metadata.get("storms_gt_2in_24h");
    assert_eq!(storms_2in.unwrap().as_i64(), Some(3));
}

#[test]
fn test_both_files_parse_successfully() {
    // Verify both sample files parse without errors
    let files = vec![
        "sample-data-files/59700_FOPR.xlsx",
        "sample-data-files/11000_FOPR.xlsx",
    ];

    for file_path in files {
        let mut workbook =
            open_workbook_auto(file_path).unwrap_or_else(|_| panic!("Failed to open {file_path}"));

        let range = workbook
            .worksheet_range("Meta_Stats")
            .unwrap_or_else(|_| panic!("Failed to find Meta_Stats in {file_path}"));

        let meta = MetaStatsData::from_worksheet_range(&range)
            .unwrap_or_else(|e| panic!("Failed to parse {file_path}: {e:?}"));

        // Basic validations for all gauges
        assert!(!meta.station_id.is_empty());
        assert!(!meta.station_name.is_empty());
        assert!(meta.latitude >= 32.0 && meta.latitude <= 34.0);
        assert!(meta.longitude >= -113.0 && meta.longitude <= -111.0);
        assert_eq!(meta.county, "Maricopa");
        assert_eq!(meta.status, "Active");

        println!("✓ Successfully parsed {} ({})", file_path, meta.station_id);
    }
}

#[test]
fn test_jsonb_serialization() {
    // Verify FOPR metadata can be serialized to JSON (for database insertion)
    let mut workbook = open_workbook_auto("sample-data-files/59700_FOPR.xlsx").unwrap();
    let range = workbook.worksheet_range("Meta_Stats").unwrap();
    let meta = MetaStatsData::from_worksheet_range(&range).unwrap();

    // Serialize fopr_metadata to JSON
    let json_string = serde_json::to_string(&meta.fopr_metadata)
        .expect("Failed to serialize FOPR metadata to JSON");

    // Verify it's valid JSON
    assert!(json_string.starts_with("{"));
    assert!(json_string.ends_with("}"));

    // Verify it contains expected keys
    assert!(json_string.contains("storms_gt_1in_24h"));
    assert!(json_string.contains("freq_15min_inches"));
    assert!(json_string.contains("freq_24hr_inches"));

    // Deserialize back
    let deserialized: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&json_string).expect("Failed to deserialize JSON");

    assert_eq!(deserialized, meta.fopr_metadata);
}
