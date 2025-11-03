use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub server_host: String,
    pub server_port: u16,
    pub gauge_url: String,
    pub fetch_interval_minutes: u64,
    pub gauge_list_url: String,
    pub gauge_list_interval_minutes: u64,
    pub fopr_worker_concurrency: usize,
}

impl Config {
    pub fn from_env() -> Result<Self, env::VarError> {
        Ok(Config {
            database_url: env::var("DATABASE_URL")?,
            server_host: env::var("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            server_port: env::var("SERVER_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .unwrap_or(8080),
            gauge_url: env::var("GAUGE_URL")?,
            fetch_interval_minutes: env::var("FETCH_INTERVAL_MINUTES")
                .unwrap_or_else(|_| "15".to_string())
                .parse()
                .unwrap_or(15),
            gauge_list_url: env::var("GAUGE_LIST_URL")?,
            gauge_list_interval_minutes: env::var("GAUGE_LIST_INTERVAL_MINUTES")
                .unwrap_or_else(|_| "60".to_string())
                .parse()
                .unwrap_or(60),
            fopr_worker_concurrency: env::var("FOPR_WORKER_CONCURRENCY")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .unwrap_or(10),
        })
    }

    pub fn server_addr(&self) -> String {
        format!("{}:{}", self.server_host, self.server_port)
    }
}
