use std::collections::BTreeSet;
#[cfg(feature = "native")]
use std::fs;
#[cfg(feature = "native")]
use std::path::PathBuf;

use serde::Serialize;
use simile_spec::{EffectOrigin, ParameterLocation, ProfileStatus, SecurityScheme};

#[cfg(feature = "native")]
use crate::Error;
use crate::Result;

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(feature = "native")]
pub struct AuditRequest {
    pub artifact: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditFinding {
    pub severity: AuditSeverity,
    pub code: String,
    pub subject: String,
    pub message: String,
    pub remediation: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditReport {
    pub service: String,
    pub service_version: String,
    pub artifact_sha256: String,
    pub operations: usize,
    pub provider_authored_outcomes: usize,
    pub provider_authored_effects: usize,
    pub convention_inferred_effects: usize,
    pub secured_operations: usize,
    pub principals: usize,
    pub webhooks: usize,
    pub signed_webhooks: bool,
    pub errors: usize,
    pub warnings: usize,
    pub findings: Vec<AuditFinding>,
}

/// Inspect where an artifact relies on convention or unconstrained synthesis.
///
/// This is deliberately an explainable audit, not a single opaque score. A
/// provider can fix each finding without learning Simile's internal compiler
/// decisions, and CI can fail only on the severities that matter to it.
#[cfg(feature = "native")]
pub fn audit(request: &AuditRequest) -> Result<AuditReport> {
    let encoded = fs::read(&request.artifact).map_err(|source| Error::ReadArtifact {
        path: request.artifact.clone(),
        source,
    })?;
    let artifact = simile_artifact::decode(&encoded)?;
    audit_artifact(&artifact)
}

/// Audit a decoded artifact without filesystem access.
pub fn audit_artifact(artifact: &simile_artifact::Artifact) -> Result<AuditReport> {
    let summary = artifact.summary();
    let mut findings = Vec::new();
    let effects = artifact.effects().collect::<Vec<_>>();
    let effected_operations = effects
        .iter()
        .flat_map(|effect| effect.operations().iter().cloned())
        .collect::<BTreeSet<_>>();
    let principal_count = artifact.principals().count();
    let profiles = artifact.profiles().collect::<Vec<_>>();
    let mut secured_operations = 0;
    for scheme in artifact.security_schemes() {
        if matches!(scheme.scheme(), SecurityScheme::MutualTls) {
            findings.push(finding(
                AuditSeverity::Error,
                "auth.mutual_tls_transport_unsupported",
                scheme.name(),
                "The HTTP listener has no verified client-certificate input.",
                "Use another declared security alternative until TLS identity reaches the server adapter.",
            ));
        }
    }

    for operation in artifact.operations() {
        let requirements = operation.security_requirements().collect::<Vec<_>>();
        let requires_authentication = !requirements.is_empty()
            && requirements
                .iter()
                .all(|requirement| !requirement.is_empty());
        if requires_authentication {
            secured_operations += 1;
            let exercised = requirements.iter().any(|requirement| {
                requirement.schemes().all(|(scheme, scopes)| {
                    let scopes = scopes.collect::<Vec<_>>();
                    profiles.iter().any(|profile| {
                        profile.status() == ProfileStatus::Active
                            && profile.credential(scheme).is_some()
                            && artifact
                                .principal(profile.principal())
                                .is_some_and(|principal| {
                                    scopes.iter().all(|scope| principal.grants(scheme, scope))
                                })
                    })
                })
            });
            if !exercised {
                findings.push(finding(
                    AuditSeverity::Warning,
                    "auth.unexercised",
                    operation.key(),
                    "No active test profile can satisfy any declared security alternative.",
                    "Declare a principal contract with the required grants and a local profile carrying its credentials.",
                ));
            }
        }
        if operation.operation_id().is_none() {
            findings.push(finding(
                AuditSeverity::Warning,
                "operation.missing_id",
                operation.key(),
                "The operation has no stable operationId.",
                "Give the operation a unique operationId so HCL behavior, effects, and scenarios can name it.",
            ));
        }
        if summary
            .operation_outcomes
            .iter()
            .any(|candidate| candidate.operation == operation.key() && candidate.underspecified)
        {
            findings.push(finding(
                AuditSeverity::Warning,
                "outcome.schema_only",
                operation.key(),
                "Every outcome is selected and synthesized from schema alone.",
                "Declare public outcomes or examples for meaningful success and error paths.",
            ));
        }
        if mutates(operation.method())
            && summary.resources > 0
            && !effected_operations.contains(operation.key())
        {
            findings.push(finding(
                AuditSeverity::Warning,
                "state.no_effect",
                operation.key(),
                "The operation appears state-mutating but commits no resource effect.",
                "Make the response entity unambiguous for convention inference or declare an HCL effect.",
            ));
        }
        for parameter in operation.parameters() {
            if parameter.schema().is_none() {
                findings.push(finding(
                    AuditSeverity::Warning,
                    "parameter.untyped",
                    &format!("{} {} {}", operation.key(), location(parameter.location()), parameter.name()),
                    "The parameter has no schema, so Simile can prove presence but not value soundness.",
                    "Add an OpenAPI schema to the parameter.",
                ));
            }
            if !supported_style(parameter.location(), parameter.style()) {
                findings.push(finding(
                    AuditSeverity::Error,
                    "parameter.unsupported_style",
                    &format!("{} {}", operation.key(), parameter.name()),
                    &format!(
                        "The runtime cannot decode the {} style at this parameter location.",
                        parameter.style()
                    ),
                    "Use a currently supported style or retain this finding until the decoder supports it.",
                ));
            }
        }
    }

    let mut used_resources = BTreeSet::new();
    for effect in &effects {
        for action in effect.actions() {
            used_resources.insert(action.resource.to_string());
        }
        if effect.origin() == EffectOrigin::ConventionInferred {
            findings.push(finding(
                AuditSeverity::Info,
                "effect.inferred",
                effect.name(),
                "Simile inferred this effect from the public operation and resource contract.",
                "Keep the convention when it matches the service, or replace it with an authored effect.",
            ));
        }
    }
    for operation in artifact.async_operations() {
        for action in operation.actions() {
            used_resources.insert(action.resource.to_string());
        }
    }
    for resource in artifact.resources() {
        let name = resource.reference().to_string();
        if !used_resources.contains(&name) {
            findings.push(finding(
                AuditSeverity::Warning,
                "backend.unused",
                &name,
                "The backend is declared but no synchronous or asynchronous action uses it.",
                "Remove the backend or connect it through an inferred or authored effect.",
            ));
        }
    }

    for cause in [
        "malformed_request",
        "invalid_body",
        "unsupported_media_type",
        "idempotency_conflict",
        "authentication_required",
        "permission_denied",
    ] {
        if artifact.protocol_error(cause).is_none() {
            findings.push(finding(
                AuditSeverity::Info,
                "error.unmapped_protocol_cause",
                cause,
                "This public protocol failure falls back to Simile's diagnostic envelope.",
                "Map the cause to a public service error when provider-faithful errors matter.",
            ));
        }
    }

    let webhook_policy = artifact.webhook_policy();
    let webhook_count = artifact.webhooks().count();
    if webhook_count > 0 && webhook_policy.signing.is_none() {
        findings.push(finding(
            AuditSeverity::Info,
            "webhook.unsigned",
            "webhooks",
            "Webhook deliveries carry no configured signature.",
            "Configure webhook signing when consumers verify provider authenticity.",
        ));
    }

    findings.sort_by(|left, right| {
        right
            .severity
            .cmp(&left.severity)
            .then_with(|| left.code.cmp(&right.code))
            .then_with(|| left.subject.cmp(&right.subject))
    });
    let errors = findings
        .iter()
        .filter(|finding| finding.severity == AuditSeverity::Error)
        .count();
    let warnings = findings
        .iter()
        .filter(|finding| finding.severity == AuditSeverity::Warning)
        .count();
    Ok(AuditReport {
        service: summary.service,
        service_version: summary.service_version,
        artifact_sha256: summary.payload_sha256,
        operations: summary.operations,
        provider_authored_outcomes: summary.provider_authored_outcomes,
        provider_authored_effects: summary.provider_authored_effects,
        convention_inferred_effects: summary.convention_inferred_effects,
        secured_operations,
        principals: principal_count,
        webhooks: webhook_count,
        signed_webhooks: webhook_policy.signing.is_some(),
        errors,
        warnings,
        findings,
    })
}

fn finding(
    severity: AuditSeverity,
    code: &str,
    subject: &str,
    message: &str,
    remediation: &str,
) -> AuditFinding {
    AuditFinding {
        severity,
        code: code.to_owned(),
        subject: subject.to_owned(),
        message: message.to_owned(),
        remediation: remediation.to_owned(),
    }
}

fn mutates(method: &str) -> bool {
    matches!(method, "POST" | "PUT" | "PATCH" | "DELETE")
}

fn supported_style(location: ParameterLocation, style: &str) -> bool {
    match location {
        // The runtime currently decodes the common OpenAPI styles below. The
        // audit must describe executable support, not merely spec validity.
        ParameterLocation::Path => matches!(style, "simple" | "label" | "matrix"),
        ParameterLocation::Query => {
            matches!(
                style,
                "form" | "spaceDelimited" | "pipeDelimited" | "deepObject"
            )
        }
        ParameterLocation::Header => style == "simple",
        ParameterLocation::Cookie => style == "form",
    }
}

const fn location(location: ParameterLocation) -> &'static str {
    match location {
        ParameterLocation::Path => "path",
        ParameterLocation::Query => "query",
        ParameterLocation::Header => "header",
        ParameterLocation::Cookie => "cookie",
    }
}
