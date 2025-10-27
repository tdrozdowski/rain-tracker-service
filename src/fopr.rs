// FOPR (Full Operational Period of Record) module
//
// This module handles importing historical rainfall data from MCFCD FOPR Excel files.
// FOPR files contain:
// - Meta_Stats sheet: Gauge metadata (location, stats, etc.)
// - Year sheets (2024, 2023, ...): Daily rainfall readings

pub mod metadata_parser;

pub use metadata_parser::{MetaStatsData, ParseError};
