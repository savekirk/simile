//! User-owned reporting consent and durable, reviewable report records.
//!
//! Service authors may name a report destination in a Simfile. They cannot enable submission or
//! choose a reporter's contact details. Those decisions live here, under `SIMILE_HOME`, beside the
//! profile that granted them.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use rand::Rng as _;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::{Error, Result};

const SETTINGS_VERSION: u16 = 1;
const REPORT_VERSION: u16 = 1;
const REPORT_FILE: &str = "report.json";
const CASE_DIRECTORY: &str = "case.simile-repro";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportingPreference {
    pub schema_version: u16,
    pub service: String,
    pub profile: String,
    pub automatic: bool,
    pub endpoint: String,
    pub contact: String,
    pub consented_at: String,
}

impl ReportingPreference {
    pub fn automatic(
        service: impl Into<String>,
        profile: impl Into<String>,
        endpoint: impl Into<String>,
        contact: impl Into<String>,
    ) -> Result<Self> {
        let endpoint = endpoint.into();
        validate_endpoint(&endpoint)?;
        let contact = contact.into();
        validate_contact(&contact)?;
        Ok(Self {
            schema_version: SETTINGS_VERSION,
            service: service.into(),
            profile: profile.into(),
            automatic: true,
            endpoint,
            contact,
            consented_at: now()?,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportKind {
    InvariantViolation,
    EmulationInconsistency,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportTrigger {
    Automatic,
    User,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportStatus {
    Prepared,
    AwaitingUpgrade,
    VerificationFailed,
    Verified,
    Submitted,
    Accepted,
    NeedsInformation,
    Resolved,
    Rejected,
}

impl ReportStatus {
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::AwaitingUpgrade => "awaiting upgrade",
            Self::VerificationFailed => "verification failed",
            Self::Verified => "verified",
            Self::Submitted => "submitted",
            Self::Accepted => "accepted",
            Self::NeedsInformation => "needs information",
            Self::Resolved => "resolved",
            Self::Rejected => "rejected",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportDiagnostic {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportVerification {
    pub checked_at: String,
    pub reproduced: bool,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeRecommendation {
    pub component: String,
    pub current: String,
    pub available: String,
    pub command: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportDocument {
    pub format_version: u16,
    pub id: String,
    pub created_at: String,
    pub kind: ReportKind,
    pub trigger: ReportTrigger,
    pub status: ReportStatus,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub service: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    pub simile_version: String,
    pub service_version: String,
    pub operation: String,
    pub artifact_digest: String,
    pub diagnostic: ReportDiagnostic,
    pub verification: ReportVerification,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub upgrade_recommendations: Vec<UpgradeRecommendation>,
    pub contactable: bool,
    pub reproduction: String,
}

impl ReportDocument {
    #[allow(clippy::too_many_arguments)]
    pub fn verified(
        kind: ReportKind,
        trigger: ReportTrigger,
        title: impl Into<String>,
        description: Option<String>,
        service: impl Into<String>,
        profile: Option<String>,
        simile_version: impl Into<String>,
        service_version: impl Into<String>,
        operation: impl Into<String>,
        artifact_digest: impl Into<String>,
        diagnostic: ReportDiagnostic,
        fingerprint: impl Into<String>,
        contactable: bool,
    ) -> Result<Self> {
        let created_at = now()?;
        Ok(Self {
            format_version: REPORT_VERSION,
            id: report_id(),
            created_at: created_at.clone(),
            kind,
            trigger,
            status: ReportStatus::Verified,
            title: title.into(),
            description,
            service: service.into(),
            profile,
            simile_version: simile_version.into(),
            service_version: service_version.into(),
            operation: operation.into(),
            artifact_digest: artifact_digest.into(),
            diagnostic,
            verification: ReportVerification {
                checked_at: created_at,
                reproduced: true,
                fingerprint: fingerprint.into(),
            },
            upgrade_recommendations: Vec::new(),
            contactable,
            reproduction: CASE_DIRECTORY.to_owned(),
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ReportingStore {
    root: PathBuf,
}

impl ReportingStore {
    pub(crate) fn new(simile_home: &Path) -> Self {
        Self {
            root: simile_home.to_owned(),
        }
    }

    pub(crate) fn save_preference(&self, preference: &ReportingPreference) -> Result<PathBuf> {
        let path = self
            .root
            .join("reporting")
            .join(&preference.service)
            .join(format!("{}.json", preference.profile));
        write_json_private(&path, preference)?;
        Ok(path)
    }

    pub(crate) fn remove_preference(&self, service: &str, profile: &str) -> Result<PathBuf> {
        let path = self
            .root
            .join("reporting")
            .join(service)
            .join(format!("{profile}.json"));
        match fs::remove_file(&path) {
            Ok(()) => Ok(path),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(path),
            Err(source) => Err(Error::ReportingIo {
                action: "remove preference",
                path,
                source,
            }),
        }
    }

    pub(crate) fn preference(
        &self,
        service: &str,
        profile: &str,
    ) -> Result<Option<ReportingPreference>> {
        let path = self
            .root
            .join("reporting")
            .join(service)
            .join(format!("{profile}.json"));
        read_json_optional(&path)
    }

    pub(crate) fn preferences(&self, service: &str) -> Result<Vec<ReportingPreference>> {
        let directory = self.root.join("reporting").join(service);
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(Error::ReportingIo {
                    action: "list preferences",
                    path: directory,
                    source,
                });
            }
        };
        let mut preferences = Vec::<ReportingPreference>::new();
        for entry in entries {
            let entry = entry.map_err(|source| Error::ReportingIo {
                action: "read preference directory",
                path: directory.clone(),
                source,
            })?;
            if entry.path().extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            preferences.push(read_json(&entry.path())?);
        }
        preferences.sort_by(|left, right| left.profile.cmp(&right.profile));
        Ok(preferences)
    }

    pub(crate) fn install_report(
        &self,
        report: &ReportDocument,
        reproduction: &Path,
    ) -> Result<PathBuf> {
        let reports = self.root.join("reports");
        create_private_directory(&reports)?;
        let destination = reports.join(&report.id);
        if destination.exists() {
            return Err(Error::ReportAlreadyExists {
                report: report.id.clone(),
            });
        }
        let staging = reports.join(format!(".{}.tmp-{}", report.id, std::process::id()));
        create_private_directory(&staging)?;
        let result = (|| {
            write_json_private(&staging.join(REPORT_FILE), report)?;
            copy_reproduction(reproduction, &staging.join(CASE_DIRECTORY))?;
            fs::rename(&staging, &destination).map_err(|source| Error::ReportingIo {
                action: "install report",
                path: destination.clone(),
                source,
            })?;
            Ok(destination.clone())
        })();
        if result.is_err() {
            drop(fs::remove_dir_all(&staging));
        }
        result
    }

    pub(crate) fn report(&self, id: &str) -> Result<(ReportDocument, PathBuf)> {
        validate_report_id(id)?;
        let directory = self.root.join("reports").join(id);
        let report = read_json(&directory.join(REPORT_FILE))?;
        Ok((report, directory))
    }

    pub(crate) fn save_report(&self, report: &ReportDocument) -> Result<()> {
        validate_report_id(&report.id)?;
        write_json_private(
            &self.root.join("reports").join(&report.id).join(REPORT_FILE),
            report,
        )
    }

    pub(crate) fn reports(&self, service: Option<&str>) -> Result<Vec<ReportDocument>> {
        let directory = self.root.join("reports");
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(Error::ReportingIo {
                    action: "list reports",
                    path: directory,
                    source,
                });
            }
        };
        let mut reports = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| Error::ReportingIo {
                action: "read reports directory",
                path: directory.clone(),
                source,
            })?;
            let path = entry.path().join(REPORT_FILE);
            if !path.is_file() {
                continue;
            }
            let report: ReportDocument = read_json(&path)?;
            if service.is_none_or(|service| report.service == service) {
                reports.push(report);
            }
        }
        reports.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        Ok(reports)
    }

    pub(crate) fn has_fingerprint(&self, service: &str, fingerprint: &str) -> Result<bool> {
        self.reports(Some(service)).map(|reports| {
            reports
                .iter()
                .any(|report| report.verification.fingerprint == fingerprint)
        })
    }
}

pub(crate) use crate::validation::validate_endpoint;

pub(crate) fn validate_contact(contact: &str) -> Result<()> {
    let contact = contact.trim();
    let Some((local, domain)) = contact.split_once('@') else {
        return Err(Error::InvalidReportContact);
    };
    if !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !contact.chars().any(char::is_whitespace)
        && contact.len() <= 254
    {
        Ok(())
    } else {
        Err(Error::InvalidReportContact)
    }
}

fn validate_report_id(id: &str) -> Result<()> {
    if id.starts_with("rpt_")
        && id.len() <= 64
        && id.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
    {
        Ok(())
    } else {
        Err(Error::InvalidReportId {
            report: id.to_owned(),
        })
    }
}

fn report_id() -> String {
    let random = rand::rng().random::<u64>();
    format!("rpt_{random:016x}")
}

fn now() -> Result<String> {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|error| Error::ReportingTime(error.to_string()))
}

fn copy_reproduction(source: &Path, destination: &Path) -> Result<()> {
    if !source.is_dir() || !source.join("bundle.json").is_file() {
        return Err(Error::InvalidReportReproduction {
            path: source.to_owned(),
        });
    }
    create_private_directory(destination)?;
    for name in [
        "bundle.json",
        "request.json",
        "request.body",
        "service.similec",
    ] {
        let source_file = source.join(name);
        if !source_file.is_file() {
            continue;
        }
        let destination_file = destination.join(name);
        let bytes = fs::read(&source_file).map_err(|source| Error::ReportingIo {
            action: "read reproduction",
            path: source_file.clone(),
            source,
        })?;
        write_private(&destination_file, &bytes)?;
    }
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let bytes = fs::read(path).map_err(|source| Error::ReportingIo {
        action: "read",
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| Error::ReportingJson {
        path: path.to_owned(),
        source,
    })
}

fn read_json_optional<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes)
                .map(Some)
                .map_err(|source| Error::ReportingJson {
                    path: path.to_owned(),
                    source,
                })
        }
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::ReportingIo {
            action: "read",
            path: path.to_owned(),
            source,
        }),
    }
}

fn write_json_private(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|source| Error::ReportingJson {
        path: path.to_owned(),
        source,
    })?;
    write_private(path, &bytes)
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or_else(|| Error::ReportingIo {
        action: "resolve parent for",
        path: path.to_owned(),
        source: std::io::Error::other("path has no parent"),
    })?;
    create_private_directory(parent)?;
    let temporary = path.with_extension(format!(
        "tmp-{}-{:016x}",
        std::process::id(),
        rand::rng().random::<u64>()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok::<(), std::io::Error>(())
    })();
    if let Err(source) = result {
        drop(fs::remove_file(&temporary));
        return Err(Error::ReportingIo {
            action: "write",
            path: path.to_owned(),
            source,
        });
    }
    Ok(())
}

fn create_private_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|source| Error::ReportingIo {
        action: "create directory",
        path: path.to_owned(),
        source,
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
            Error::ReportingIo {
                action: "secure directory",
                path: path.to_owned(),
                source,
            }
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn accepts_secure_and_loopback_endpoints_only() {
        assert!(validate_endpoint("https://reports.example.test/v1/reports").is_ok());
        assert!(validate_endpoint("http://127.0.0.1:8787/v1/reports").is_ok());
        assert!(validate_endpoint("http://reports.example.test/v1/reports").is_err());
        assert!(validate_endpoint("https://reports.example.test/").is_err());
        assert!(validate_endpoint("https://reports.example.test/v1/reports?token=no").is_err());
    }

    #[test]
    fn validates_contact_without_pretending_to_deliver_mail() {
        assert!(validate_contact("reporter@example.test").is_ok());
        assert!(validate_contact("missing-domain").is_err());
        assert!(validate_contact("spaces are@example.test").is_err());
    }

    #[test]
    fn stores_reporting_consent_privately_and_per_profile() {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "simile-reporting-store-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let store = ReportingStore::new(&root);
        let preference = ReportingPreference::automatic(
            "inttegro",
            "checkout",
            "https://reports.example.test/v1/reports",
            "reporter@example.test",
        )
        .unwrap_or_else(|error| panic!("preference must be valid: {error}"));

        let path = store
            .save_preference(&preference)
            .unwrap_or_else(|error| panic!("preference must be stored: {error}"));
        let loaded = store
            .preference("inttegro", "checkout")
            .unwrap_or_else(|error| panic!("preference must be readable: {error}"));

        assert_eq!(loaded.as_ref(), Some(&preference));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = fs::metadata(path)
                .unwrap_or_else(|error| panic!("preference metadata must exist: {error}"))
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        #[cfg(not(unix))]
        drop(path);
        drop(fs::remove_dir_all(root));
    }
}
