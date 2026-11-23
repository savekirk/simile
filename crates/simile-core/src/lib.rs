#![forbid(unsafe_code)]

mod build_info;
#[cfg(feature = "native")]
mod context;
mod error;
pub mod ops;
#[cfg(feature = "native")]
mod registry;
#[cfg(feature = "native")]
mod reporting;
#[cfg(feature = "native")]
mod service_config;

pub use build_info::BuildInfo;
#[cfg(feature = "native")]
pub use context::Context;
pub use error::{Error, Result};
#[cfg(feature = "native")]
pub use reporting::{
    ReportDiagnostic, ReportDocument, ReportKind, ReportStatus, ReportTrigger, ReportVerification,
    ReportingPreference, UpgradeRecommendation,
};

pub mod session;
mod sources;
mod validation;
pub mod workspace;
