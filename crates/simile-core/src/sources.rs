use crate::{Error, Result};
use serde::Serialize;
use simile_compiler::{ContractSource, Source};
use std::collections::BTreeSet;
#[cfg(feature = "native")]
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) trait Sources {
    fn identity(&self, path: &Path) -> Result<PathBuf>;
    fn read(&self, path: &Path) -> Result<String>;
    fn is_file(&self, path: &Path) -> bool;
    fn is_dir(&self, path: &Path) -> bool;
    fn declarations(&self, path: &Path) -> Result<Vec<PathBuf>>;
}

#[cfg(feature = "native")]
pub(crate) struct NativeSources;
#[cfg(feature = "native")]
impl Sources for NativeSources {
    fn identity(&self, path: &Path) -> Result<PathBuf> {
        fs::canonicalize(path).map_err(|source| Error::ReadSource {
            path: path.to_owned(),
            source,
        })
    }
    fn read(&self, path: &Path) -> Result<String> {
        fs::read_to_string(path).map_err(|source| Error::ReadSource {
            path: path.to_owned(),
            source,
        })
    }
    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }
    fn is_dir(&self, path: &Path) -> bool {
        path.is_dir()
    }
    fn declarations(&self, path: &Path) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        collect_declarations(path, &mut paths)?;
        Ok(paths)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, serde::Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractFormat {
    #[default]
    Auto,
    #[serde(rename = "openapi")]
    OpenApi,
    Smithy,
    #[serde(rename = "typescript")]
    TypeScript,
}

impl std::fmt::Display for ContractFormat {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Auto => "auto",
            Self::OpenApi => "OpenAPI",
            Self::Smithy => "Smithy",
            Self::TypeScript => "TypeScript",
        })
    }
}

#[derive(Debug)]
pub(crate) struct LoadedContract {
    pub(crate) format: ContractFormat,
    pub(crate) sources: Vec<(String, String)>,
}

pub(crate) fn load_contract(
    provider: &impl Sources,
    path: &Path,
    requested: ContractFormat,
) -> Result<LoadedContract> {
    let format = infer_contract_format(provider, path, requested)?;
    let source_paths = if format == ContractFormat::TypeScript {
        let mut paths = Vec::new();
        if provider.is_dir(path) {
            paths = provider.declarations(path)?;
        } else {
            collect_declaration_graph(provider, path, &mut BTreeSet::new(), &mut paths)?;
        }
        paths.sort();
        paths
    } else {
        vec![path.to_owned()]
    };
    if source_paths.is_empty() {
        return Err(Error::EmptyTypeScriptContract {
            path: path.to_owned(),
        });
    }
    let sources = source_paths
        .iter()
        .map(|path| Ok((path.to_string_lossy().into_owned(), provider.read(path)?)))
        .collect::<Result<Vec<_>>>()?;
    Ok(LoadedContract { format, sources })
}

fn collect_declaration_graph(
    provider: &impl Sources,
    path: &Path,
    visited: &mut BTreeSet<PathBuf>,
    target: &mut Vec<PathBuf>,
) -> Result<()> {
    let path = provider.identity(path)?;
    if !visited.insert(path.clone()) {
        return Ok(());
    }
    let contents = provider.read(&path)?;
    let name = path.to_string_lossy();
    let specifiers =
        simile_typescript::module_specifiers(simile_typescript::Source::new(&name, &contents))
            .map_err(simile_compiler::Error::TypeScript)?;
    for specifier in specifiers {
        if !specifier.starts_with('.') {
            continue;
        }
        let imported =
            resolve_declaration_import(provider, &path, &specifier).ok_or_else(|| {
                Error::UnresolvedTypeScriptModule {
                    importer: path.clone(),
                    specifier: specifier.clone(),
                }
            })?;
        collect_declaration_graph(provider, &imported, visited, target)?;
    }
    target.push(path);
    Ok(())
}

fn resolve_declaration_import(
    provider: &impl Sources,
    importer: &Path,
    specifier: &str,
) -> Option<PathBuf> {
    let parent = importer.parent().unwrap_or_else(|| Path::new("."));
    let base = parent.join(specifier);
    let candidates = [
        base.clone(),
        base.with_extension("d.ts"),
        base.join("index.d.ts"),
    ];
    candidates
        .into_iter()
        .find(|candidate| provider.is_file(candidate))
}

pub(crate) fn compiler_source(contract: &LoadedContract) -> ContractSource<'_> {
    match contract.format {
        ContractFormat::OpenApi | ContractFormat::Auto => {
            let (name, contents) = &contract.sources[0];
            ContractSource::OpenApi(Source::new(name, contents))
        }
        ContractFormat::Smithy => {
            let (name, contents) = &contract.sources[0];
            ContractSource::Smithy(Source::new(name, contents))
        }
        ContractFormat::TypeScript => ContractSource::TypeScript(
            contract
                .sources
                .iter()
                .map(|(name, contents)| Source::new(name, contents))
                .collect(),
        ),
    }
}

fn infer_contract_format(
    provider: &impl Sources,
    path: &Path,
    requested: ContractFormat,
) -> Result<ContractFormat> {
    if requested != ContractFormat::Auto {
        return Ok(requested);
    }
    if provider.is_dir(path) || path.to_string_lossy().ends_with(".d.ts") {
        return Ok(ContractFormat::TypeScript);
    }
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("smithy") => Ok(ContractFormat::Smithy),
        Some("json" | "yaml" | "yml") => Ok(ContractFormat::OpenApi),
        _ => Err(Error::UnknownContractFormat {
            path: path.to_owned(),
        }),
    }
}

#[cfg(feature = "native")]
fn collect_declarations(directory: &Path, target: &mut Vec<PathBuf>) -> Result<()> {
    let entries = fs::read_dir(directory).map_err(|source| Error::ReadSource {
        path: directory.to_owned(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| Error::ReadSource {
            path: directory.to_owned(),
            source,
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_declarations(&path, target)?;
            continue;
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if name.ends_with(".d.ts") && !name.ends_with(".test.d.ts") {
            target.push(path);
        }
    }
    Ok(())
}
