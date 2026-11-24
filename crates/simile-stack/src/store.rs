#[cfg(feature = "persistence")]
use std::ffi::OsString;
#[cfg(feature = "persistence")]
use std::path::Path;
use std::path::PathBuf;

#[cfg(feature = "process")]
use sha2::{Digest as _, Sha256};
#[cfg(feature = "process")]
use simile_spec::ResourceKind;
use thiserror::Error;

#[cfg(feature = "process")]
use crate::ResourceAddress;
use crate::StackSnapshot;

mod memory;
#[cfg(feature = "persistence")]
mod redb;

pub use memory::MemoryStateStore;
#[cfg(feature = "persistence")]
pub use redb::RedbStateStore;

pub type StateStoreResult<T> = std::result::Result<T, StateStoreError>;

/// Provenance attached when a component is owned by a compiled service.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateIdentity {
    pub service_identity: String,
    pub artifact_digest: String,
}

impl StateIdentity {
    #[must_use]
    pub fn new(service_identity: impl Into<String>, artifact_digest: impl Into<String>) -> Self {
        Self {
            service_identity: service_identity.into(),
            artifact_digest: artifact_digest.into(),
        }
    }
}

#[cfg(feature = "process")]
pub(crate) fn default_component_state_path<'a>(
    kind: ResourceKind,
    resources: impl IntoIterator<Item = &'a ResourceAddress>,
) -> StateStoreResult<PathBuf> {
    Ok(component_state_path(&simile_home()?, kind, resources))
}

#[cfg(feature = "process")]
fn component_state_path<'a>(
    simile_home: &Path,
    kind: ResourceKind,
    resources: impl IntoIterator<Item = &'a ResourceAddress>,
) -> PathBuf {
    let mut fingerprint = Sha256::new();
    fingerprint.update(b"simile-stack-state-v1\0");
    fingerprint.update(kind.to_string().as_bytes());
    let mut resources = resources
        .into_iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    resources.sort();
    for resource in resources {
        fingerprint.update(b"\0");
        fingerprint.update(resource.as_bytes());
    }
    let fingerprint = format!("{:x}", fingerprint.finalize());
    simile_home
        .join("state")
        .join("stack")
        .join(kind.to_string())
        .join(format!("{fingerprint}.redb"))
}

#[cfg(feature = "process")]
fn simile_home() -> StateStoreResult<PathBuf> {
    if let Some(path) = std::env::var_os("SIMILE_HOME").filter(|path| !path.is_empty()) {
        let path = PathBuf::from(path);
        return if path.is_absolute() {
            Ok(path)
        } else {
            Ok(std::env::current_dir()?.join(path))
        };
    }
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .map(|home| home.join(".simile"))
        .ok_or(StateStoreError::Home)
}

/// Durable boundary for one complete Stack state.
///
/// Stores receive a storage-neutral snapshot. A memory store may retain it as
/// one value; a durable store may project it into independently keyed records.
/// Resource engines remain unaware of either strategy.
pub trait StateStore: Send + Sync {
    fn load(&self) -> StateStoreResult<Option<StackSnapshot>>;
    fn save(&self, snapshot: &StackSnapshot) -> StateStoreResult<()>;

    fn is_durable(&self) -> bool {
        false
    }
}

#[derive(Debug, Error)]
pub enum StateStoreError {
    #[error("state storage failed: {0}")]
    Database(String),
    #[error("could not decode persisted state: {0}")]
    Decode(String),
    #[error("could not encode persisted state: {0}")]
    Encode(String),
    #[error("persisted state format {found} is incompatible with supported format {supported}")]
    Format { found: u16, supported: u16 },
    #[error("could not determine the user home directory; set SIMILE_HOME explicitly")]
    Home,
    #[error("state storage I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("state file `{path}` is already in use")]
    Locked {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("the in-memory state store lock is unavailable")]
    Poisoned,
    #[error("persisted state identity does not match {field} `{expected}`")]
    Identity {
        field: &'static str,
        expected: String,
    },
    #[error("persisted protocol {found} is incompatible with supported protocol {supported}")]
    Protocol { found: u16, supported: u16 },
}

#[cfg(feature = "persistence")]
fn lock_path(path: &Path) -> PathBuf {
    let mut value = OsString::from(path.as_os_str());
    value.push(".lock");
    PathBuf::from(value)
}

#[cfg(all(test, feature = "process"))]
mod tests {
    use std::path::Path;

    use simile_spec::ResourceKind;

    use super::component_state_path;
    use crate::ResourceAddress;

    #[test]
    fn automatic_paths_are_stable_across_resource_argument_order()
    -> Result<(), Box<dyn std::error::Error>> {
        let home = Path::new("simile-home");
        let first = ResourceAddress::new(ResourceKind::Queue, "fulfillment")?;
        let second = ResourceAddress::new(ResourceKind::Queue, "receipts")?;
        let forward = component_state_path(home, ResourceKind::Queue, [&first, &second]);
        let reverse = component_state_path(home, ResourceKind::Queue, [&second, &first]);

        assert_eq!(forward, reverse);
        assert!(forward.starts_with(home.join("state").join("stack").join("queue")));
        Ok(())
    }
}
