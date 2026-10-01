//! Kadı Atlas — read-only research API over a corpus of historical Ottoman
//! kadı (judge) appointment records.
//!
//! The public HTTP surface (see [`app`] and [`api`]) is strictly read-only.
//! Data is loaded exclusively through the `import_xlsx` CLI ([`importer`]).

pub mod api;
pub mod app;
pub mod config;
pub mod db;
pub mod error;
pub mod importer;
pub mod normalize;

pub use config::Config;
pub use error::{ApiError, ApiResult};
