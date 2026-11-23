use std::net::SocketAddr;

use serde::Serialize;
use simile_artifact::Artifact;
use simile_spec::{
    BehaviorOrigin, DescriptionKind, FaultAction, FaultPhase, ProfileStatus, SetupBinding,
    SetupIssueSource, SetupValueType, WebhookOrigin,
};

#[cfg(feature = "native")]
use super::start::prepare_loaded_service;
#[cfg(feature = "native")]
use crate::registry::Registry;
#[cfg(feature = "native")]
use crate::service_config::{LoadedServiceConfig, ServiceConfigSource};
#[cfg(feature = "native")]
use crate::{Context, Result};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DescribeServiceRequest {
    pub service: Option<String>,
    pub config: Option<String>,
    pub verbose: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribeServiceReport {
    pub service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub service_version: String,
    pub openapi_version: String,
    pub listen: SocketAddr,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reporting_endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    pub operation_count: usize,
    pub operations: Vec<DescribedOperation>,
    pub scenarios: Vec<DescribedScenario>,
    pub faults: Vec<DescribedFault>,
    pub fault_profiles: Vec<DescribedFaultProfile>,
    pub backends: Vec<DescribedBackend>,
    pub async_operations: Vec<DescribedAsyncOperation>,
    pub protocol_errors: Vec<DescribedProtocolError>,
    pub webhooks: DescribedWebhookPolicy,
    pub webhook_contracts: Vec<DescribedWebhook>,
    pub authoring_objects: Vec<DescribedObject>,
    pub principals: Vec<DescribedPrincipal>,
    pub profiles: Vec<DescribedProfile>,
    pub principal_setups: Vec<DescribedSetup>,
}

/// Artifact catalogue with no invented listener or filesystem source.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactDescription {
    pub service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub service_version: String,
    pub openapi_version: String,
    pub operation_count: usize,
    pub operations: Vec<DescribedOperation>,
    pub scenarios: Vec<DescribedScenario>,
    pub faults: Vec<DescribedFault>,
    pub fault_profiles: Vec<DescribedFaultProfile>,
    pub backends: Vec<DescribedBackend>,
    pub async_operations: Vec<DescribedAsyncOperation>,
    pub protocol_errors: Vec<DescribedProtocolError>,
    pub webhooks: DescribedWebhookPolicy,
    pub webhook_contracts: Vec<DescribedWebhook>,
    pub authoring_objects: Vec<DescribedObject>,
    pub principals: Vec<DescribedPrincipal>,
    pub profiles: Vec<DescribedProfile>,
    pub principal_setups: Vec<DescribedSetup>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribedSetup {
    pub principal: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub operation_id: String,
    pub collects: Vec<DescribedSetupCollect>,
    pub issues: Vec<DescribedSetupIssue>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribedSetupCollect {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub required: bool,
    pub value_type: String,
    pub request_pointer: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribedSetupIssue {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
    pub sensitive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub binding: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribedWebhookPolicy {
    pub ordering: String,
    pub out_of_order_probability_ppm: u32,
    pub reorder_window_milliseconds: u64,
    pub concurrency: u16,
    pub signed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature_header: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribedWebhook {
    pub name: String,
    pub method: String,
    pub origin: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub content_types: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribedOperation {
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub behavior_source: String,
    pub secured: bool,
    pub outcomes: Vec<DescribedOutcome>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribedOutcome {
    pub name: String,
    pub status: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribedScenario {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub selections: usize,
    pub faults: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribedFault {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub phase: String,
    pub action: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribedFaultProfile {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub probability_ppm: u32,
    pub faults: Vec<String>,
    pub operations: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribedBackend {
    pub reference: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribedAsyncOperation {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub backends: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribedProtocolError {
    pub cause: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub statuses: Vec<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribedObject {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub name: String,
    pub description: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribedPrincipal {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub security_schemes: Vec<String>,
    pub grants: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescribedProfile {
    pub name: String,
    pub principal: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub status: String,
    pub security_schemes: Vec<String>,
}

#[cfg(feature = "native")]
pub fn describe_service(
    ctx: &Context,
    request: &DescribeServiceRequest,
) -> Result<DescribeServiceReport> {
    let (service, artifact, listen, source, reporting_endpoint, revision) =
        match (&request.service, &request.config) {
            (requested, Some(config)) => {
                let loaded = LoadedServiceConfig::load(
                    ctx,
                    ServiceConfigSource::from_argument(config.clone()),
                )?;
                let prepared =
                    prepare_loaded_service(requested.as_deref(), None, None, &loaded)?.service;
                (
                    prepared.report.service,
                    prepared.artifact,
                    prepared.report.listen,
                    config.clone(),
                    prepared.reporting_endpoint,
                    None,
                )
            }
            (Some(service), None) => {
                let registered = Registry::new(ctx.simile_home()?).load(service)?;
                let listen = SocketAddr::new(
                    registered.manifest.listen.host,
                    registered.manifest.listen.port,
                );
                let document = simile_hcl::parse_service(simile_hcl::Source::new(
                    &registered.manifest.name,
                    &registered.config_contents,
                ))?;
                (
                    registered.manifest.name,
                    registered.artifact,
                    listen,
                    "registry".to_owned(),
                    document
                        .service
                        .reporting
                        .map(|reporting| reporting.endpoint),
                    Some(registered.revision),
                )
            }
            (None, None) => {
                let loaded = LoadedServiceConfig::load_simfile(ctx)?;
                let prepared = prepare_loaded_service(None, None, None, &loaded)?.service;
                (
                    prepared.report.service,
                    prepared.artifact,
                    prepared.report.listen,
                    loaded.name,
                    prepared.reporting_endpoint,
                    None,
                )
            }
        };

    Ok(report(
        &service,
        request.verbose,
        &artifact,
        listen,
        source,
        reporting_endpoint,
        revision,
    ))
}

#[cfg(feature = "native")]
fn report(
    service: &str,
    verbose: bool,
    artifact: &Artifact,
    listen: SocketAddr,
    source: String,
    reporting_endpoint: Option<String>,
    revision: Option<String>,
) -> DescribeServiceReport {
    let catalogue = describe_artifact(artifact, service, verbose);
    DescribeServiceReport {
        listen,
        source,
        reporting_endpoint,
        revision,
        service: catalogue.service,
        description: catalogue.description,
        service_version: catalogue.service_version,
        openapi_version: catalogue.openapi_version,
        operation_count: catalogue.operation_count,
        operations: catalogue.operations,
        scenarios: catalogue.scenarios,
        faults: catalogue.faults,
        fault_profiles: catalogue.fault_profiles,
        backends: catalogue.backends,
        async_operations: catalogue.async_operations,
        protocol_errors: catalogue.protocol_errors,
        webhooks: catalogue.webhooks,
        webhook_contracts: catalogue.webhook_contracts,
        authoring_objects: catalogue.authoring_objects,
        principals: catalogue.principals,
        profiles: catalogue.profiles,
        principal_setups: catalogue.principal_setups,
    }
}

/// Describe an artifact using an optional host-supplied service label.
pub fn describe_artifact(artifact: &Artifact, service: &str, verbose: bool) -> ArtifactDescription {
    let summary = artifact.summary();
    let operations = if verbose {
        artifact
            .operations()
            .map(|operation| {
                let operation_id = operation.operation_id().map(str::to_owned);
                let description = operation_id
                    .as_deref()
                    .and_then(|id| artifact.description(DescriptionKind::Behavior, None, id))
                    .map(str::to_owned)
                    .or_else(|| operation.summary().map(str::to_owned));
                let outcome_parent = operation_id.as_deref().map(|id| format!("behavior:{id}"));
                let outcomes = operation
                    .outcomes()
                    .map(|outcome| DescribedOutcome {
                        name: outcome.name().to_owned(),
                        status: outcome.status(),
                        description: outcome_parent
                            .as_deref()
                            .and_then(|parent| {
                                artifact.description(
                                    DescriptionKind::Outcome,
                                    Some(parent),
                                    outcome.name(),
                                )
                            })
                            .map(str::to_owned),
                    })
                    .collect();
                DescribedOperation {
                    key: operation.key().to_owned(),
                    operation_id,
                    description,
                    behavior_source: match operation.origin() {
                        BehaviorOrigin::OpenApi => "contract_derived",
                        BehaviorOrigin::Hcl => "provider_authored",
                    }
                    .to_owned(),
                    secured: {
                        let requirements = operation.security_requirements().collect::<Vec<_>>();
                        !requirements.is_empty()
                            && requirements
                                .iter()
                                .all(|requirement| !requirement.is_empty())
                    },
                    outcomes,
                }
            })
            .collect()
    } else {
        Vec::new()
    };

    let scenarios = artifact
        .scenarios()
        .map(|scenario| DescribedScenario {
            name: scenario.name().to_owned(),
            description: description(artifact, DescriptionKind::Scenario, scenario.name()),
            selections: scenario.selections().len(),
            faults: scenario.faults().len(),
        })
        .collect();
    let faults = artifact
        .faults()
        .map(|fault| DescribedFault {
            name: fault.name().to_owned(),
            description: description(artifact, DescriptionKind::Fault, fault.name()),
            phase: match fault.phase() {
                FaultPhase::BeforeResponse => "before_response",
                FaultPhase::AfterCommit => "after_commit",
            }
            .to_owned(),
            action: match fault.action() {
                FaultAction::Delay { milliseconds } => format!("delay {milliseconds}ms"),
                FaultAction::Disconnect => "disconnect".to_owned(),
            },
        })
        .collect();
    let fault_profiles = artifact
        .fault_profiles()
        .map(|profile| DescribedFaultProfile {
            name: profile.name().to_owned(),
            description: description(artifact, DescriptionKind::FaultProfile, profile.name()),
            probability_ppm: profile.probability_ppm(),
            faults: profile.faults().to_vec(),
            operations: profile.operations().to_vec(),
        })
        .collect();
    let backends = artifact
        .resources()
        .map(|resource| {
            let reference = resource.reference().to_string();
            DescribedBackend {
                description: description(artifact, DescriptionKind::Backend, &reference),
                reference,
            }
        })
        .collect();
    let async_operations = artifact
        .async_operations()
        .map(|operation| DescribedAsyncOperation {
            name: operation.name().to_owned(),
            description: description(artifact, DescriptionKind::AsyncOperation, operation.name()),
            backends: operation
                .resources()
                .map(|resource| resource.reference().to_string())
                .collect(),
        })
        .collect();
    let protocol_errors = artifact
        .protocol_errors()
        .map(|error| DescribedProtocolError {
            cause: error.cause().to_owned(),
            description: description(artifact, DescriptionKind::ProtocolError, error.cause()),
            statuses: error.statuses().to_vec(),
        })
        .collect();
    let authoring_objects = artifact
        .descriptions()
        .filter(|entry| {
            matches!(
                entry.kind(),
                DescriptionKind::ErrorSet | DescriptionKind::Outcome
            )
        })
        .map(|entry| DescribedObject {
            kind: kind_name(entry.kind()).to_owned(),
            parent: entry.parent().map(str::to_owned),
            name: entry.name().to_owned(),
            description: entry.text().to_owned(),
        })
        .collect();
    let webhook_policy = artifact.webhook_policy();
    let webhook_contracts = artifact
        .webhooks()
        .map(|webhook| DescribedWebhook {
            name: webhook.name().to_owned(),
            method: webhook.method().to_owned(),
            origin: match webhook.origin() {
                WebhookOrigin::OpenApiCallback => "openapi_callback",
                WebhookOrigin::OpenApiWebhook => "openapi_webhook",
                WebhookOrigin::ProviderAuthored => "provider_authored",
            }
            .to_owned(),
            operation_id: webhook.operation_id().map(str::to_owned),
            description: description(artifact, DescriptionKind::Webhook, webhook.name())
                .or_else(|| webhook.summary().map(str::to_owned)),
            target: webhook.target().map(str::to_owned),
            content_types: webhook
                .request_body()
                .map(|body| body.media_types().map(str::to_owned).collect())
                .unwrap_or_default(),
        })
        .collect();
    let principals = artifact
        .principals()
        .map(|principal| DescribedPrincipal {
            name: principal.name().to_owned(),
            description: description(artifact, DescriptionKind::Principal, principal.name()),
            security_schemes: principal.credential_schemes().map(str::to_owned).collect(),
            grants: principal.granted_scopes().map(str::to_owned).collect(),
        })
        .collect();
    let profiles = artifact
        .profiles()
        .map(|profile| DescribedProfile {
            name: profile.name().to_owned(),
            principal: profile.principal().to_owned(),
            description: description(artifact, DescriptionKind::Profile, profile.name()),
            status: match profile.status() {
                ProfileStatus::Active => "active",
                ProfileStatus::Revoked => "revoked",
                ProfileStatus::Expired => "expired",
            }
            .to_owned(),
            security_schemes: profile.credential_schemes().map(str::to_owned).collect(),
        })
        .collect();
    let principal_setups = artifact
        .principal_setups()
        .map(|setup| DescribedSetup {
            principal: setup.principal,
            description: setup.description,
            operation_id: setup.operation_id,
            collects: setup
                .collects
                .into_values()
                .map(|collect| DescribedSetupCollect {
                    name: collect.name,
                    description: collect.description,
                    required: collect.required,
                    value_type: setup_value_type_name(collect.value_type).to_owned(),
                    request_pointer: collect.request_pointer,
                    binding: collect.binding.map(setup_binding_name).map(str::to_owned),
                })
                .collect(),
            issues: setup
                .issues
                .into_values()
                .map(|issue| DescribedSetupIssue {
                    name: issue.name,
                    description: issue.description,
                    source: match issue.source {
                        SetupIssueSource::Response { pointer } => format!("response{pointer}"),
                        SetupIssueSource::Token { prefix } if prefix.is_empty() => {
                            "generated token".to_owned()
                        }
                        SetupIssueSource::Token { prefix } => {
                            format!("generated token with prefix {prefix}")
                        }
                    },
                    credential: issue.credential,
                    sensitive: issue.sensitive,
                    binding: issue.binding.map(setup_binding_name).map(str::to_owned),
                })
                .collect(),
        })
        .collect();

    ArtifactDescription {
        service: service.to_owned(),
        description: artifact
            .description(DescriptionKind::Service, None, service)
            .map(str::to_owned)
            .or_else(|| artifact.service_description().map(str::to_owned)),
        service_version: summary.service_version,
        openapi_version: summary.openapi_version,
        operation_count: summary.operations,
        operations,
        scenarios,
        faults,
        fault_profiles,
        backends,
        async_operations,
        protocol_errors,
        webhooks: DescribedWebhookPolicy {
            ordering: webhook_policy.ordering.as_str().to_owned(),
            out_of_order_probability_ppm: webhook_policy.out_of_order_probability_ppm,
            reorder_window_milliseconds: webhook_policy.reorder_window_milliseconds,
            concurrency: webhook_policy.concurrency,
            signed: webhook_policy.signing.is_some(),
            signature_header: webhook_policy
                .signing
                .as_ref()
                .map(|signing| signing.header.clone()),
        },
        webhook_contracts,
        authoring_objects,
        principals,
        profiles,
        principal_setups,
    }
}

const fn setup_value_type_name(value_type: SetupValueType) -> &'static str {
    match value_type {
        SetupValueType::String => "string",
        SetupValueType::Url => "url",
        SetupValueType::Boolean => "boolean",
        SetupValueType::Integer => "integer",
        SetupValueType::Json => "json",
    }
}

const fn setup_binding_name(binding: SetupBinding) -> &'static str {
    match binding {
        SetupBinding::WebhookTarget => "webhook.target",
        SetupBinding::WebhookSigningSecret => "webhook.signing_secret",
    }
}

fn description(artifact: &Artifact, kind: DescriptionKind, name: &str) -> Option<String> {
    artifact.description(kind, None, name).map(str::to_owned)
}

const fn kind_name(kind: DescriptionKind) -> &'static str {
    match kind {
        DescriptionKind::Service => "service",
        DescriptionKind::Behavior => "behavior",
        DescriptionKind::Outcome => "outcome",
        DescriptionKind::ErrorSet => "error_set",
        DescriptionKind::ProtocolError => "protocol_error",
        DescriptionKind::Scenario => "scenario",
        DescriptionKind::Backend => "backend",
        DescriptionKind::AsyncOperation => "async",
        DescriptionKind::Fault => "fault",
        DescriptionKind::FaultProfile => "fault_profile",
        DescriptionKind::Effect => "effect",
        DescriptionKind::Webhook => "webhook",
        DescriptionKind::Principal => "principal",
        DescriptionKind::Profile => "profile",
        DescriptionKind::PrincipalSetup => "principal_setup",
    }
}
