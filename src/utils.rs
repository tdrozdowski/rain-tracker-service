/// Shared utility functions for the rain tracker service
///
/// Extract 4-5 digit station ID from a string that may contain additional text
///
/// Station IDs in the MCFCD system are either 4 or 5 digits. Sometimes they appear
/// with additional text like "29200 since 03/09/18" or "40700 since 6/30/20".
/// This function extracts just the numeric ID portion.
///
/// # Examples
///
/// ```
/// use rain_tracker_service::utils::extract_station_id;
///
/// assert_eq!(extract_station_id("29200").unwrap(), "29200");
/// assert_eq!(extract_station_id("1800 since 03/27/18").unwrap(), "1800");
/// assert_eq!(extract_station_id("40700 since 6/30/20").unwrap(), "40700");
/// assert_eq!(extract_station_id("37300 since installation").unwrap(), "37300");
/// ```
pub fn extract_station_id(value: &str) -> Result<String, &'static str> {
    // Find first whitespace-delimited token that is 4-5 digits
    for part in value.split_whitespace() {
        let len = part.len();
        if (len == 4 || len == 5) && part.chars().all(|c| c.is_ascii_digit()) {
            return Ok(part.to_string());
        }
    }

    // Fallback: extract leading digits if they are 4-5 in length
    let digits: String = value.chars().take_while(|c| c.is_ascii_digit()).collect();
    let len = digits.len();
    if len == 4 || len == 5 {
        return Ok(digits);
    }

    Err("No valid 4-5 digit station ID found")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_station_id_5digit_clean() {
        assert_eq!(extract_station_id("29200").unwrap(), "29200");
    }

    #[test]
    fn test_extract_station_id_4digit_clean() {
        assert_eq!(extract_station_id("1800").unwrap(), "1800");
    }

    #[test]
    fn test_extract_station_id_with_since_5digit() {
        assert_eq!(extract_station_id("29200 since 03/09/18").unwrap(), "29200");
    }

    #[test]
    fn test_extract_station_id_with_since_4digit() {
        assert_eq!(extract_station_id("1800 since 03/27/18").unwrap(), "1800");
    }

    #[test]
    fn test_extract_station_id_with_installation() {
        assert_eq!(
            extract_station_id("37300 since installation").unwrap(),
            "37300"
        );
    }

    #[test]
    fn test_extract_station_id_with_prior() {
        assert_eq!(
            extract_station_id("4695 prior to 2/20/2018").unwrap(),
            "4695"
        );
    }

    #[test]
    fn test_extract_station_id_too_short() {
        assert!(extract_station_id("123").is_err());
    }

    #[test]
    fn test_extract_station_id_too_long() {
        assert!(extract_station_id("123456").is_err());
    }

    #[test]
    fn test_extract_station_id_non_numeric() {
        assert!(extract_station_id("ABCDE").is_err());
    }

    #[test]
    fn test_extract_station_id_leading_digits() {
        // "1234A" should extract "1234" as leading digits
        assert_eq!(extract_station_id("1234A").unwrap(), "1234");
    }
}
