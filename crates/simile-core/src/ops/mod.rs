mod audit;
#[cfg(feature = "native")]
mod compile;
mod conformance;
mod describe;
#[cfg(feature = "native")]
mod doctor;
#[cfg(feature = "native")]
mod inspect;
#[cfg(feature = "native")]
mod profile;
#[cfg(feature = "native")]
mod proxy;
#[cfg(feature = "native")]
mod register;
#[cfg(feature = "native")]
mod remove;
#[cfg(feature = "native")]
mod report;
mod reproduce;
mod run;
#[cfg(feature = "native")]
mod start;
mod state;
#[cfg(feature = "native")]
mod test;
#[cfg(feature = "native")]
mod update;
#[cfg(feature = "native")]
mod version;

#[cfg(feature = "native")]
pub use audit::{AuditRequest, audit};
#[cfg(feature = "native")]
pub use compile::{CompileReport, CompileRequest, ContractFormat, WrittenArtifact, compile};
#[cfg(feature = "native")]
pub use conformance::{ConformanceCase, ConformanceReport, ConformanceRequest, verify_conformance};
#[cfg(feature = "native")]
pub use describe::{
    DescribeServiceReport, DescribeServiceRequest, DescribedAsyncOperation, DescribedBackend,
    DescribedFault, DescribedFaultProfile, DescribedObject, DescribedOperation, DescribedOutcome,
    DescribedProtocolError, DescribedScenario, DescribedSetup, DescribedSetupCollect,
    DescribedSetupIssue, describe_service,
};
#[cfg(feature = "native")]
pub use doctor::{Check, CheckStatus, DoctorReport, doctor};
#[cfg(feature = "native")]
pub use inspect::{InspectRequest, inspect};
#[cfg(feature = "native")]
pub(crate) use profile::profile_environment;
#[cfg(feature = "native")]
pub use profile::{
    ProfileReport, ProfilesReport, SetupProfileReport, SetupProfileRequest, profile, profiles,
    setup_contract, setup_profile, use_profile,
};
#[cfg(feature = "native")]
pub use proxy::{ProxyPlan, ProxyReport, ProxyRequest, prepare_proxy};
#[cfg(feature = "native")]
pub use register::{RegisterRequest, RegistrationReport, register};
#[cfg(feature = "native")]
pub use remove::{RemoveReport, RemoveRequest, remove};
#[cfg(feature = "native")]
pub use report::{
    AutomaticReportRequest, ConfigureReportingReport, ConfigureReportingRequest,
    PrepareReportRequest, PreparedReport, ReportingPreferenceSummary, ReportsReport,
    SendReportReport, SendReportRequest, configure_reporting, prepare_report,
    process_automatic_report, report, reporting_destination, reporting_preferences, reports,
    send_report,
};
#[cfg(feature = "native")]
pub use reproduce::{ReplayReport, ReplayRequest, replay};
#[cfg(feature = "native")]
pub use run::{
    ExecutedRequest, ExecutedResponse, ExecutedWebhook, OperationSelector, RequestInput,
    RequestSource, RunReport, RunRequest, SpecSource, run,
};
#[cfg(feature = "native")]
pub use start::{PreparedService, ServiceReport, StartRequest, prepare_service};
#[cfg(feature = "native")]
pub use state::{StateReport, StateRequest, inspect_state, service_state_path};
#[cfg(feature = "native")]
pub use test::{TestPlan, TestRequest, prepare_test};
#[cfg(feature = "native")]
pub use update::{UpdateRequest, update};
#[cfg(feature = "native")]
pub use version::version;

// Shared application entry points, also available without native host facilities.
pub use audit::{AuditFinding, AuditReport, AuditSeverity, audit_artifact};
pub use conformance::verify_bundles;
pub use reproduce::{MemoryBundle, replay_bundle, response_bundle, verify_bundle};
pub use run::{BaselineRequest, run_artifact};
pub use state::inspect_snapshot;

#[cfg(not(feature = "native"))]
pub use conformance::{ConformanceCase, ConformanceReport};
pub use describe::{ArtifactDescription, describe_artifact};
#[cfg(not(feature = "native"))]
pub use reproduce::ReplayReport;
#[cfg(not(feature = "native"))]
pub use run::{
    ExecutedRequest, ExecutedResponse, ExecutedWebhook, OperationSelector, RequestSource, RunReport,
};
#[cfg(not(feature = "native"))]
pub use state::{StateReport, StateRequest};
