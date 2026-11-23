use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use simile_artifact::Artifact;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::{Error, Result};

const LAYOUT_VERSION: &str = "1";
const MANIFEST_VERSION: u16 = 1;
const CONFIG_FILE: &str = "config.hcl";
const ARTIFACT_FILE: &str = "service.similec";
const MANIFEST_FILE: &str = "manifest.json";
const CURRENT_FILE: &str = "current";

#[derive(Clone, Debug)]
pub(crate) struct Registry {
    root: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct RegistrationInput<'a> {
    pub service: &'a str,
    pub config: &'a str,
    pub config_source: SourceLocation,
    pub openapi_source: SourceLocation,
    pub listen: SocketAddr,
    pub artifact: &'a Artifact,
    pub artifact_bytes: &'a [u8],
    pub replace: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct RegisteredService {
    pub revision: String,
    pub manifest: RegistrationManifest,
    pub artifact: Artifact,
    pub artifact_bytes: Vec<u8>,
    pub config_contents: String,
    pub directory: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RegistrationManifest {
    pub schema_version: u16,
    pub name: String,
    pub registered_at: String,
    pub config_source: SourceLocation,
    pub config: String,
    pub artifact: String,
    pub listen: RegisteredListen,
    pub openapi_source: SourceLocation,
    pub summary: RegisteredSummary,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RegisteredListen {
    pub host: IpAddr,
    pub port: u16,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub(crate) enum SourceLocation {
    Path { value: String },
    Url { value: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RegisteredSummary {
    pub artifact_format: u16,
    pub compiler_version: String,
    pub service_version: String,
    pub openapi_version: String,
    pub operations: usize,
    pub resources: usize,
    pub async_operations: usize,
}

impl Registry {
    pub(crate) fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub(crate) fn ensure_registration_available(&self, service: &str, replace: bool) -> Result<()> {
        validate_service_name(service)?;
        if self.service_directory(service).join(CURRENT_FILE).exists() && !replace {
            Err(Error::ServiceAlreadyRegistered {
                service: service.to_owned(),
            })
        } else {
            Ok(())
        }
    }

    pub(crate) fn register(&self, input: RegistrationInput<'_>) -> Result<RegisteredService> {
        validate_service_name(input.service)?;
        self.ensure_layout()?;

        let service_directory = self.service_directory(input.service);
        let current = service_directory.join(CURRENT_FILE);
        self.ensure_registration_available(input.service, input.replace)?;

        let revision = revision_id();
        let staging = self
            .root
            .join("tmp")
            .join(format!("{}-{revision}", input.service));
        let revision_directory = service_directory.join("revisions").join(&revision);
        create_private_directory(&staging)?;

        let result = (|| {
            write_private(&staging.join(CONFIG_FILE), input.config.as_bytes(), false)?;
            write_private(&staging.join(ARTIFACT_FILE), input.artifact_bytes, false)?;

            let summary = input.artifact.summary();
            let registered_at = OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .map_err(|error| Error::InvalidRegistration {
                    service: input.service.to_owned(),
                    reason: format!("could not format registration time: {error}"),
                })?;
            let manifest = RegistrationManifest {
                schema_version: MANIFEST_VERSION,
                name: input.service.to_owned(),
                registered_at,
                config_source: input.config_source,
                config: CONFIG_FILE.to_owned(),
                artifact: ARTIFACT_FILE.to_owned(),
                listen: RegisteredListen {
                    host: input.listen.ip(),
                    port: input.listen.port(),
                },
                openapi_source: input.openapi_source,
                summary: RegisteredSummary {
                    artifact_format: summary.format_version,
                    compiler_version: summary.compiler_version,
                    service_version: summary.service_version,
                    openapi_version: summary.openapi_version,
                    operations: summary.operations,
                    resources: summary.resources,
                    async_operations: summary.async_operations,
                },
            };
            let manifest_bytes =
                serde_json::to_vec_pretty(&manifest).map_err(|source| Error::RegistryManifest {
                    path: staging.join(MANIFEST_FILE),
                    source,
                })?;
            write_private(&staging.join(MANIFEST_FILE), &manifest_bytes, false)?;

            create_private_directory(&service_directory)?;
            create_private_directory(&service_directory.join("revisions"))?;
            fs::rename(&staging, &revision_directory).map_err(|source| Error::RegistryIo {
                action: "install service revision",
                path: revision_directory.clone(),
                source,
            })?;
            write_private(&current, format!("{revision}\n").as_bytes(), input.replace)?;

            Ok(RegisteredService {
                revision: revision.clone(),
                manifest,
                artifact: input.artifact.clone(),
                artifact_bytes: input.artifact_bytes.to_vec(),
                config_contents: input.config.to_owned(),
                directory: revision_directory.clone(),
            })
        })();

        if result.is_err() {
            drop(fs::remove_dir_all(&staging));
            drop(fs::remove_dir_all(&revision_directory));
        }
        result
    }

    pub(crate) fn load(&self, service: &str) -> Result<RegisteredService> {
        validate_service_name(service)?;
        if !self.root.join("layout-version").exists() {
            return Err(Error::ServiceNotRegistered {
                service: service.to_owned(),
            });
        }
        self.verify_layout()?;
        let service_directory = self.service_directory(service);
        let current_path = service_directory.join(CURRENT_FILE);
        let revision = match fs::read_to_string(&current_path) {
            Ok(value) => value.trim().to_owned(),
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::ServiceNotRegistered {
                    service: service.to_owned(),
                });
            }
            Err(source) => {
                return Err(Error::RegistryIo {
                    action: "read current service revision",
                    path: current_path,
                    source,
                });
            }
        };
        if !valid_revision(&revision) {
            return Err(Error::InvalidRegistration {
                service: service.to_owned(),
                reason: "current revision is not a safe path component".to_owned(),
            });
        }

        let directory = service_directory.join("revisions").join(&revision);
        let manifest_path = directory.join(MANIFEST_FILE);
        let manifest_bytes = read(&manifest_path, "read service manifest")?;
        let manifest: RegistrationManifest =
            serde_json::from_slice(&manifest_bytes).map_err(|source| Error::RegistryManifest {
                path: manifest_path,
                source,
            })?;
        validate_manifest(service, &manifest)?;

        let artifact_path = directory.join(ARTIFACT_FILE);
        let artifact_bytes = read(&artifact_path, "read registered artifact")?;
        let artifact = simile_artifact::decode(&artifact_bytes)?;
        validate_artifact(service, &manifest, &artifact)?;
        let config_path = directory.join(CONFIG_FILE);
        let config_bytes = read(&config_path, "read registered service configuration")?;
        let config_contents =
            String::from_utf8(config_bytes).map_err(|source| Error::InvalidRegistration {
                service: service.to_owned(),
                reason: format!("stored service configuration is not UTF-8: {source}"),
            })?;

        Ok(RegisteredService {
            revision,
            manifest,
            artifact,
            artifact_bytes,
            config_contents,
            directory,
        })
    }

    pub(crate) fn remove(&self, service: &str) -> Result<PathBuf> {
        validate_service_name(service)?;
        if !self.root.join("layout-version").exists() {
            return Err(Error::ServiceNotRegistered {
                service: service.to_owned(),
            });
        }
        self.verify_layout()?;

        let directory = self.service_directory(service);
        let current = directory.join(CURRENT_FILE);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_file() => {}
            Ok(_) => {
                return Err(Error::InvalidRegistration {
                    service: service.to_owned(),
                    reason: "current revision pointer is not a regular file".to_owned(),
                });
            }
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::ServiceNotRegistered {
                    service: service.to_owned(),
                });
            }
            Err(source) => {
                return Err(Error::RegistryIo {
                    action: "inspect current service revision",
                    path: current,
                    source,
                });
            }
        }

        fs::remove_dir_all(&directory).map_err(|source| Error::RegistryIo {
            action: "remove registered service",
            path: directory.clone(),
            source,
        })?;
        Ok(directory)
    }

    fn ensure_layout(&self) -> Result<()> {
        create_private_directory(&self.root)?;
        create_private_directory(&self.root.join("services"))?;
        create_private_directory(&self.root.join("locks"))?;
        create_private_directory(&self.root.join("tmp"))?;
        let version = self.root.join("layout-version");
        if version.exists() {
            self.verify_layout()
        } else {
            write_private(&version, format!("{LAYOUT_VERSION}\n").as_bytes(), false)
        }
    }

    fn verify_layout(&self) -> Result<()> {
        let path = self.root.join("layout-version");
        let version = match fs::read_to_string(&path) {
            Ok(value) => value,
            Err(source) => {
                return Err(Error::RegistryIo {
                    action: "read registry layout version",
                    path,
                    source,
                });
            }
        };
        if version.trim() == LAYOUT_VERSION {
            Ok(())
        } else {
            Err(Error::UnsupportedRegistryLayout {
                version: version.trim().to_owned(),
                path,
            })
        }
    }

    fn service_directory(&self, service: &str) -> PathBuf {
        self.root.join("services").join(service)
    }
}

fn validate_manifest(service: &str, manifest: &RegistrationManifest) -> Result<()> {
    if manifest.schema_version != MANIFEST_VERSION {
        return Err(Error::InvalidRegistration {
            service: service.to_owned(),
            reason: format!(
                "manifest schema version {} is unsupported",
                manifest.schema_version
            ),
        });
    }
    if manifest.name != service {
        return Err(Error::InvalidRegistration {
            service: service.to_owned(),
            reason: format!("manifest names `{}`", manifest.name),
        });
    }
    if manifest.config != CONFIG_FILE || manifest.artifact != ARTIFACT_FILE {
        return Err(Error::InvalidRegistration {
            service: service.to_owned(),
            reason: "manifest contains unsupported managed file names".to_owned(),
        });
    }
    Ok(())
}

fn validate_artifact(
    service: &str,
    manifest: &RegistrationManifest,
    artifact: &Artifact,
) -> Result<()> {
    let summary = artifact.summary();
    let expected = &manifest.summary;
    if summary.format_version != expected.artifact_format
        || summary.compiler_version != expected.compiler_version
        || summary.service_version != expected.service_version
        || summary.openapi_version != expected.openapi_version
        || summary.operations != expected.operations
        || summary.resources != expected.resources
        || summary.async_operations != expected.async_operations
    {
        return Err(Error::InvalidRegistration {
            service: service.to_owned(),
            reason: "artifact does not match its manifest summary".to_owned(),
        });
    }
    Ok(())
}

pub(crate) use crate::validation::validate_service_name;

fn valid_revision(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .chars()
            .all(|character| character.is_ascii_digit() || character == '-')
}

fn revision_id() -> String {
    static REVISION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    format!(
        "{}-{}-{}",
        OffsetDateTime::now_utc().unix_timestamp_nanos(),
        std::process::id(),
        REVISION_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

fn create_private_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|source| Error::RegistryIo {
        action: "create directory",
        path: path.to_owned(),
        source,
    })?;
    set_private_directory_permissions(path)
}

fn write_private(path: &Path, contents: &[u8], replace: bool) -> Result<()> {
    static WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    let sequence = WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_extension(format!("{}.{sequence}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temporary)?;
        file.write_all(contents)?;
        file.sync_all()?;
        if !replace && path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "destination already exists",
            ));
        }
        if replace && path.exists() {
            #[cfg(windows)]
            fs::remove_file(path)?;
        }
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        drop(fs::remove_file(&temporary));
    }
    result.map_err(|source| Error::RegistryIo {
        action: "write file",
        path: path.to_owned(),
        source,
    })
}

fn read(path: &Path, action: &'static str) -> Result<Vec<u8>> {
    fs::read(path).map_err(|source| Error::RegistryIo {
        action,
        path: path.to_owned(),
        source,
    })
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|source| {
        Error::RegistryIo {
            action: "set directory permissions",
            path: path.to_owned(),
            source,
        }
    })
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> Result<()> {
    Ok(())
}
