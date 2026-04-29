#![forbid(unsafe_code)]

pub mod action;
pub mod agent;
pub mod error;
pub mod policy;
pub mod traits;
pub mod types;

pub use error::{PatchwrightError, Result};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
