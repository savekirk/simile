//! Resolve a virtual project and compose it through the same compiler as native hosts.
use std::collections::BTreeMap;
use std::net::{IpAddr, SocketAddr};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use simile_compiler::{Compiler, Source, SourceCompileRequest};
use simile_hcl::{ContractDeclaration, SourceDeclaration};
use simile_heuristics::Policy;

pub use crate::sources::ContractFormat;
use crate::sources::{Sources, compiler_source, load_contract};
use crate::{Error, Result};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceRequest {
    pub files: BTreeMap<String, String>,
    pub config_path: Option<String>,
    pub contract_entry: Option<String>,
    #[serde(default)]
    pub contract_format: ContractFormat,
    #[serde(default)]
    pub modules: Vec<String>,
    #[serde(default)]
    pub heuristic_policy: Policy,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMetadata {
    pub service: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listen: Option<SocketAddr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reporting_endpoint: Option<String>,
}

pub struct CompiledWorkspace {
    pub artifact: simile_artifact::Artifact,
    pub bytes: Vec<u8>,
    pub metadata: WorkspaceMetadata,
}

/// A Simfile is authoritative. Without one, the caller selects the contract and modules.
pub fn compile_workspace(request: &WorkspaceRequest) -> Result<CompiledWorkspace> {
    let files = MemorySources::new(&request.files)?;
    let config = request.config_path.as_deref().or_else(|| {
        (request.contract_entry.is_none() && files.0.contains_key("Simfile")).then_some("Simfile")
    });
    let (compiled, metadata) = if let Some(config) = config {
        if request.contract_entry.is_some()
            || !request.modules.is_empty()
            || request.contract_format != ContractFormat::Auto
            || request.heuristic_policy != Policy::Standard
        {
            return Err(Error::Workspace(
                "a Simfile cannot be combined with contract, module, or heuristic overrides"
                    .to_owned(),
            ));
        }
        let config = normalize(Path::new(config))?;
        let text = files.read(Path::new(&config))?;
        let document = simile_hcl::parse_service(simile_hcl::Source::new(&config, &text))?;
        crate::validation::validate_service_name(&document.service.name)?;
        let reporting_endpoint = document
            .service
            .reporting
            .as_ref()
            .map(|value| value.endpoint.clone());
        if let Some(endpoint) = &reporting_endpoint {
            crate::validation::validate_endpoint(endpoint)?;
        }
        let host = document
            .service
            .listen
            .host
            .parse::<IpAddr>()
            .map_err(|source| Error::InvalidListenHost {
                host: document.service.listen.host.clone(),
                source,
            })?;
        let (declaration, format) = match &document.service.contract {
            ContractDeclaration::OpenApi(source) => (source, ContractFormat::OpenApi),
            ContractDeclaration::Smithy(source) => (source, ContractFormat::Smithy),
            ContractDeclaration::TypeScript(source) => (source, ContractFormat::TypeScript),
        };
        let SourceDeclaration::Path(entry) = declaration else {
            return Err(Error::Workspace(
                "remote contracts are unavailable in a virtual workspace; add the source as a file"
                    .to_owned(),
            ));
        };
        let parent = Path::new(&config).parent().unwrap_or_else(|| Path::new(""));
        let contract = load_contract(&files, &parent.join(entry), format)?;
        let modules = document
            .service
            .modules
            .iter()
            .map(|module| {
                let path = normalize(&parent.join(module))?;
                simile_hcl::parse(simile_hcl::Source::new(
                    &path,
                    &files.read(Path::new(&path))?,
                ))
                .map_err(Into::into)
            })
            .collect::<Result<Vec<_>>>()?;
        let compiled = Compiler::new().compile_service(
            &document,
            compiler_source(&contract),
            modules,
            Policy::Standard,
        )?;
        let metadata = WorkspaceMetadata {
            service: document.service.name,
            source: config,
            listen: Some(SocketAddr::new(host, document.service.listen.port)),
            reporting_endpoint,
        };
        (compiled, metadata)
    } else {
        let entry = request
            .contract_entry
            .as_deref()
            .ok_or_else(|| Error::Workspace("select a Simfile or contract entry".to_owned()))?;
        let entry = normalize(Path::new(entry))?;
        let contract = load_contract(&files, Path::new(&entry), request.contract_format)?;
        let modules = request
            .modules
            .iter()
            .map(|name| {
                let name = normalize(Path::new(name))?;
                let contents = files.read(Path::new(&name))?;
                Ok((name, contents))
            })
            .collect::<Result<Vec<_>>>()?;
        let compiled = Compiler::new().compile_source(SourceCompileRequest {
            contract: compiler_source(&contract),
            modules: modules
                .iter()
                .map(|(name, contents)| Source::new(name, contents))
                .collect(),
            heuristic_policy: request.heuristic_policy,
        })?;
        let metadata = WorkspaceMetadata {
            service: compiled.contract.info.title.clone(),
            source: entry,
            listen: None,
            reporting_endpoint: None,
        };
        (compiled, metadata)
    };
    let bytes = simile_artifact::encode(&compiled)?;
    let artifact = simile_artifact::decode(&bytes)?;
    Ok(CompiledWorkspace {
        artifact,
        bytes,
        metadata,
    })
}

struct MemorySources(BTreeMap<String, String>);
impl MemorySources {
    fn new(files: &BTreeMap<String, String>) -> Result<Self> {
        let mut normalized = BTreeMap::new();
        for (path, contents) in files {
            let path = normalize(Path::new(path))?;
            if normalized.insert(path.clone(), contents.clone()).is_some() {
                return Err(Error::Workspace(format!(
                    "duplicate normalized file `{path}`"
                )));
            }
        }
        Ok(Self(normalized))
    }
}
impl Sources for MemorySources {
    fn identity(&self, path: &Path) -> Result<PathBuf> {
        normalize(path).map(PathBuf::from)
    }
    fn read(&self, path: &Path) -> Result<String> {
        let name = normalize(path)?;
        self.0.get(&name).cloned().ok_or_else(|| Error::ReadSource {
            path: path.to_owned(),
            source: std::io::Error::from(std::io::ErrorKind::NotFound),
        })
    }
    fn is_file(&self, path: &Path) -> bool {
        normalize(path).is_ok_and(|path| self.0.contains_key(&path))
    }
    fn is_dir(&self, path: &Path) -> bool {
        normalize(path).is_ok_and(|path| {
            self.0
                .keys()
                .any(|name| path.is_empty() || name.starts_with(&format!("{path}/")))
        })
    }
    fn declarations(&self, path: &Path) -> Result<Vec<PathBuf>> {
        let prefix = normalize(path)?;
        Ok(self
            .0
            .keys()
            .filter(|name| {
                (prefix.is_empty() || name.starts_with(&format!("{prefix}/")))
                    && name.ends_with(".d.ts")
                    && !name.ends_with(".test.d.ts")
            })
            .map(PathBuf::from)
            .collect())
    }
}

fn normalize(path: &Path) -> Result<String> {
    let text = path.to_string_lossy();
    if text.contains(['\\', ':', '\0']) {
        return Err(Error::Workspace(format!("invalid virtual path `{text}`")));
    }
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => parts.push(value.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir if !parts.is_empty() => {
                parts.pop();
            }
            _ => {
                return Err(Error::Workspace(format!(
                    "path `{text}` escapes the virtual workspace"
                )));
            }
        }
    }
    Ok(parts.join("/"))
}
