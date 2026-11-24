use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use simile_artifact::ArtifactSummary;
use simile_compiler::{Compiler, Source, SourceCompileRequest};
use simile_heuristics::Policy;
use simile_spec::{BehaviorOrigin, CompiledService};

use crate::{Error, Result};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileRequest {
    pub contract: PathBuf,
    pub contract_format: ContractFormat,
    pub modules: Vec<PathBuf>,
    pub artifact: Option<PathBuf>,
    pub heuristic_policy: Policy,
}

pub use crate::sources::ContractFormat;
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WrittenArtifact {
    pub path: PathBuf,
    pub format_version: u16,
    pub bytes: usize,
    pub payload_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompileReport {
    pub service: String,
    pub service_version: String,
    pub contract_format: ContractFormat,
    /// JSON Schema feature line used in the compiled artifact.
    pub openapi_version: String,
    pub operations: usize,
    pub openapi_defaults: usize,
    pub hcl_behaviors: usize,
    pub async_operations: usize,
    pub backends: usize,
    pub webhooks: usize,
    pub webhook_emissions: usize,
    pub faults: usize,
    pub fault_profiles: usize,
    pub heuristic_policy: Policy,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact: Option<WrittenArtifact>,
}

/// Compile a public contract and zero or more HCL modules.
pub fn compile(request: &CompileRequest) -> Result<CompileReport> {
    let loaded = load_contract(&request.contract, request.contract_format)?;
    let compiled = compile_loaded(&loaded, &request.modules, request.heuristic_policy)?;
    let hcl_behaviors = compiled
        .operations
        .values()
        .filter(|operation| operation.origin == BehaviorOrigin::Hcl)
        .count();
    let artifact = request
        .artifact
        .as_ref()
        .map(|path| write_compiled_artifact(path, &compiled))
        .transpose()?;

    Ok(CompileReport {
        service: compiled.contract.info.title.clone(),
        service_version: compiled.contract.info.version.clone(),
        contract_format: loaded.format,
        openapi_version: compiled.contract.openapi.exact.clone(),
        operations: compiled.operations.len(),
        openapi_defaults: compiled.operations.len() - hcl_behaviors,
        hcl_behaviors,
        async_operations: compiled.async_operations.len(),
        backends: compiled.resources.len(),
        webhooks: compiled.webhooks.len(),
        webhook_emissions: compiled
            .effects
            .values()
            .map(|effect| effect.webhooks.len())
            .sum(),
        faults: compiled.faults.len(),
        fault_profiles: compiled.fault_profiles.len(),
        heuristic_policy: compiled.heuristic_policy,
        artifact,
    })
}

pub(crate) fn compile_contract_path(
    path: &Path,
    format: ContractFormat,
    module_paths: &[PathBuf],
    heuristic_policy: Policy,
) -> Result<CompiledService> {
    let loaded = load_contract(path, format)?;
    compile_loaded(&loaded, module_paths, heuristic_policy)
}

pub(crate) fn compile_contract_source(
    name: &str,
    contents: &str,
    format: ContractFormat,
    module_paths: &[PathBuf],
    heuristic_policy: Policy,
) -> Result<CompiledService> {
    let loaded = LoadedContract {
        format,
        sources: vec![(name.to_owned(), contents.to_owned())],
    };
    compile_loaded(&loaded, module_paths, heuristic_policy)
}

fn compile_loaded(
    contract: &LoadedContract,
    module_paths: &[PathBuf],
    heuristic_policy: Policy,
) -> Result<CompiledService> {
    let module_contents = module_paths
        .iter()
        .map(|path| read(path))
        .collect::<Result<Vec<_>>>()?;
    let module_names = module_paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let modules = module_names
        .iter()
        .zip(&module_contents)
        .map(|(name, contents)| Source::new(name, contents))
        .collect();
    Compiler::new()
        .compile_source(SourceCompileRequest {
            contract: compiler_source(contract),
            modules,
            heuristic_policy,
        })
        .map_err(Into::into)
}

pub(crate) use crate::sources::{LoadedContract, compiler_source};
pub(crate) fn load_contract(path: &Path, requested: ContractFormat) -> Result<LoadedContract> {
    crate::sources::load_contract(&crate::sources::NativeSources, path, requested)
}

fn write_compiled_artifact(
    path: &Path,
    compiled: &simile_spec::CompiledService,
) -> Result<WrittenArtifact> {
    let encoded = simile_artifact::encode(compiled)?;
    let summary = simile_artifact::decode(&encoded)?.summary();
    write_atomic(path, &encoded)?;
    Ok(written_artifact(path, &summary))
}

fn written_artifact(path: &Path, summary: &ArtifactSummary) -> WrittenArtifact {
    WrittenArtifact {
        path: path.to_owned(),
        format_version: summary.format_version,
        bytes: summary.encoded_bytes,
        payload_sha256: summary.payload_sha256.clone(),
    }
}

static WRITE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact.similec");
    let sequence = WRITE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{file_name}.{}.{}.tmp",
        std::process::id(),
        sequence
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        drop(fs::remove_file(&temporary));
    }
    result.map_err(|source| Error::WriteArtifact {
        path: path.to_owned(),
        source,
    })
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|source| Error::ReadSource {
        path: path.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn reports_compiled_units() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let report = compile(&CompileRequest {
            contract: fixtures.join("orders.yaml"),
            contract_format: ContractFormat::Auto,
            modules: vec![fixtures.join("orders.hcl")],
            artifact: None,
            heuristic_policy: Policy::Standard,
        })
        .unwrap_or_else(|error| panic!("{error}"));

        assert_eq!(report.service, "Orders");
        assert_eq!(report.openapi_version, "3.2.1");
        assert_eq!(report.operations, 1);
        assert_eq!(report.hcl_behaviors, 1);
        assert_eq!(report.openapi_defaults, 0);
        assert_eq!(report.async_operations, 1);
        assert_eq!(report.backends, 1);
        assert_eq!(report.webhooks, 0);
        assert_eq!(report.webhook_emissions, 0);
        assert_eq!(report.artifact, None);
    }

    #[test]
    fn writes_a_loadable_artifact() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
        let artifact = std::env::temp_dir().join(format!(
            "simile-core-compile-{}.similec",
            std::process::id()
        ));
        drop(fs::remove_file(&artifact));

        let report = compile(&CompileRequest {
            contract: fixtures.join("orders.yaml"),
            contract_format: ContractFormat::Auto,
            modules: vec![fixtures.join("orders.hcl")],
            artifact: Some(artifact.clone()),
            heuristic_policy: Policy::Standard,
        })
        .unwrap_or_else(|error| panic!("{error}"));
        let bytes = fs::read(&artifact).unwrap_or_else(|error| panic!("{error}"));
        let decoded = simile_artifact::decode(&bytes).unwrap_or_else(|error| panic!("{error}"));

        assert_eq!(decoded.summary().service, "Orders");
        assert_eq!(
            report.artifact.as_ref().map(|report| report.path.as_path()),
            Some(artifact.as_path())
        );
        fs::remove_file(&artifact).unwrap_or_else(|error| panic!("{error}"));
    }

    #[test]
    fn loads_only_the_typescript_entrypoints_import_graph() {
        let directory = std::env::temp_dir().join(format!(
            "simile-core-typescript-imports-{}",
            std::process::id()
        ));
        drop(fs::remove_dir_all(&directory));
        fs::create_dir_all(&directory).unwrap_or_else(|error| panic!("{error}"));
        let entry = directory.join("commerce.d.ts");
        fs::write(&entry, "export * from \"./orders\";\n")
            .unwrap_or_else(|error| panic!("{error}"));
        fs::write(
            directory.join("orders.d.ts"),
            r#"
export type ServiceName = "Orders";
export interface OrdersOperations {
  readonly listOrders: {
    readonly method: "GET";
    readonly path: "/orders";
    readonly responses: { readonly "200": { readonly content: {
      readonly "application/json": { readonly count: number };
    } } };
  };
}
"#,
        )
        .unwrap_or_else(|error| panic!("{error}"));
        fs::write(
            directory.join("unrelated.d.ts"),
            "export interface BrokenOperations {}\n",
        )
        .unwrap_or_else(|error| panic!("{error}"));

        let loaded =
            load_contract(&entry, ContractFormat::Auto).unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(loaded.format, ContractFormat::TypeScript);
        assert_eq!(loaded.sources.len(), 2);
        let compiled = compile_loaded(&loaded, &[], Policy::Standard)
            .unwrap_or_else(|error| panic!("{error}"));
        assert!(compiled.contract.operation_by_id("listOrders").is_some());

        fs::remove_dir_all(&directory).unwrap_or_else(|error| panic!("{error}"));
    }
}
