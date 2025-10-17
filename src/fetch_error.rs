#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Failed to parse HTML data")]
    ParseError,
    #[error("Failed to parse date/time: {0}")]
    DateTimeError(String),
    #[error("Failed to parse number: {0}")]
    NumberError(String),
}
