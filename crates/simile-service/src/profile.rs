use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use rand::{Rng as _, SeedableRng as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use simile_artifact::ArtifactOperation;
use simile_runtime::{
    ExecutionOptions, OperationExecution, OperationValueCall, SynthesisRequest,
    synthesize_minimal_request_value,
};
use simile_spec::{
    PrincipalSetup, Profile, ProfileCredential, ProfileStatus, ResponseStatus, SetupBinding,
    SetupIssueSource, SetupValueType, WebhookSigning,
};
use simile_stack::{
    CommandMetadata, DbCommand, DbResult, ResourceAddress, Stack, StackCommand, StackResult,
    WriteCondition,
};

use crate::{Error, Result, Service};

pub(crate) const PROFILE_STATE: &str = "__simile_profiles";
const PROFILES: &str = "profiles";
const SETTINGS: &str = "settings";
const CURRENT_PROFILE: &str = "current_profile";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileValue {
    pub value: Value,
    pub sensitive: bool,
}

/// One durable consumer identity configured for a service.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceProfile {
    pub id: String,
    pub name: String,
    pub principal: String,
    pub status: ProfileStatus,
    pub collected: BTreeMap<String, Value>,
    pub issued: BTreeMap<String, ProfileValue>,
    pub credentials: BTreeMap<String, ProfileCredential>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_signing_secret: Option<String>,
    pub service_revision: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl ServiceProfile {
    #[must_use]
    pub fn credential_profile(&self) -> Profile {
        Profile {
            name: self.name.clone(),
            principal: self.principal.clone(),
            status: self.status,
            credentials: self.credentials.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceProfileSummary {
    pub id: String,
    pub name: String,
    pub principal: String,
    pub status: ProfileStatus,
    pub issued: Vec<ProfileFieldSummary>,
    pub credential_schemes: Vec<String>,
    pub webhook_target_configured: bool,
    pub webhook_signing_configured: bool,
    pub current: bool,
    pub service_revision: String,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileFieldSummary {
    pub name: String,
    pub sensitive: bool,
}

impl ServiceProfileSummary {
    fn from_profile(profile: &ServiceProfile, current: Option<&str>) -> Self {
        Self {
            id: profile.id.clone(),
            name: profile.name.clone(),
            principal: profile.principal.clone(),
            status: profile.status,
            issued: profile
                .issued
                .iter()
                .map(|(name, value)| ProfileFieldSummary {
                    name: name.clone(),
                    sensitive: value.sensitive,
                })
                .collect(),
            credential_schemes: profile.credentials.keys().cloned().collect(),
            webhook_target_configured: profile.webhook_target.is_some(),
            webhook_signing_configured: profile.webhook_signing_secret.is_some(),
            current: current == Some(profile.name.as_str()),
            service_revision: profile.service_revision.clone(),
            created_at: profile.created_at,
            updated_at: profile.updated_at,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SetupProfileResult {
    pub profile: ServiceProfile,
    pub created: bool,
    pub selected: bool,
}

impl Service {
    /// Create or reconfigure one profile from a compiled principal-setup workflow.
    pub fn setup_profile(
        &self,
        principal: Option<&str>,
        name: Option<&str>,
        supplied: BTreeMap<String, Value>,
        select: bool,
    ) -> Result<SetupProfileResult> {
        self.setup_profile_with_seed(principal, name, supplied, select, rand::random())
    }

    /// Reproduce setup values, credentials, and profile identifiers for a fixture.
    pub fn setup_profile_with_seed(
        &self,
        principal: Option<&str>,
        name: Option<&str>,
        supplied: BTreeMap<String, Value>,
        select: bool,
        seed: u64,
    ) -> Result<SetupProfileResult> {
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
        let definition = resolve_principal_setup(self, principal)?;
        let name = match name {
            Some(name) => {
                validate_profile_name(name)?;
                name.to_owned()
            }
            None => self.available_profile_name(&mut rng)?,
        };
        reject_unknown_values(&definition, &supplied)?;
        let existing = self.profile(&name)?;
        let mut collected = existing
            .as_ref()
            .map_or_else(BTreeMap::new, |profile| profile.collected.clone());
        collected.extend(supplied);
        validate_collected(&definition, &collected)?;

        if let Some(mut profile) = existing {
            if profile.principal != definition.principal {
                return Err(Error::ProfilePrincipalMismatch {
                    profile: profile.name,
                    expected: profile.principal,
                    actual: definition.principal,
                });
            }
            apply_collected_bindings(&definition, &collected, &mut profile)?;
            profile.collected = collected;
            profile.updated_at = self.clock()?.now.as_unix_millis();
            let mut stack = super::lock(&self.stack)?;
            let before = stack.snapshot()?;
            put_profile(&mut stack, &profile)?;
            let selected = select || current_profile_name(&mut stack)?.is_none();
            if selected {
                put_current_profile(&mut stack, &profile.name)?;
            }
            self.checkpoint(&mut stack, before)?;
            return Ok(SetupProfileResult {
                profile,
                created: false,
                selected,
            });
        }

        let operation = self
            .artifact()
            .operation_by_id(&definition.operation_id)
            .ok_or_else(|| Error::SetupOperationMissing(definition.operation_id.clone()))?;
        let (content_type, mut request) = match operation.request_body() {
            Some(_) => {
                let synthesized = synthesize_minimal_request_value(
                    self.artifact(),
                    SynthesisRequest {
                        operation: operation.key(),
                        content_type: None,
                    },
                )?;
                let (content_type, value) = synthesized.into_parts();
                (Some(content_type), Some(value))
            }
            None => (None, None),
        };
        if !collected.is_empty() && request.is_none() {
            return Err(Error::SetupRequestBodyMissing {
                operation: definition.operation_id.clone(),
            });
        }
        if let Some(request) = &mut request {
            for (collect, value) in definition
                .collects
                .values()
                .filter_map(|collect| collected.get(&collect.name).map(|value| (collect, value)))
            {
                set_pointer(request, &collect.request_pointer, value.clone())?;
            }
        }
        let execution = self.runtime.execute_value_with_options(
            OperationValueCall {
                operation: operation.key(),
                content_type: content_type.as_deref(),
                body: request.as_ref(),
            },
            ExecutionOptions {
                response_status: successful_response_status(operation),
                response_seed: Some(rng.random()),
                ..ExecutionOptions::default()
            },
        )?;
        let mut profile = build_profile(self, &name, &definition, collected, &execution, &mut rng)?;
        let collected = profile.collected.clone();
        apply_collected_bindings(&definition, &collected, &mut profile)?;

        let mut stack = super::lock(&self.stack)?;
        let before = stack.snapshot()?;
        let selected = select || current_profile_name(&mut stack)?.is_none();
        let result = (|| {
            self.apply_commit(&mut stack, &execution, None, Some(&profile))?;
            if selected {
                put_current_profile(&mut stack, &profile.name)?;
            }
            Ok::<(), Error>(())
        })();
        if let Err(error) = result {
            *stack = Stack::from_snapshot(before);
            return Err(error);
        }
        self.checkpoint(&mut stack, before)?;
        Ok(SetupProfileResult {
            profile,
            created: true,
            selected,
        })
    }

    pub fn profiles(&self) -> Result<Vec<ServiceProfileSummary>> {
        let mut stack = super::lock(&self.stack)?;
        let current = current_profile_name(&mut stack)?;
        profiles(&mut stack).map(|profiles| {
            profiles
                .iter()
                .map(|profile| ServiceProfileSummary::from_profile(profile, current.as_deref()))
                .collect()
        })
    }

    fn available_profile_name(&self, rng: &mut impl rand::Rng) -> Result<String> {
        let existing = self
            .profile_records()?
            .into_iter()
            .map(|profile| profile.name)
            .collect::<BTreeSet<_>>();
        crate::profile_name::generate_with_rng(rng, &existing)
            .ok_or(Error::ProfileNameSpaceExhausted)
    }

    pub(super) fn profile_records(&self) -> Result<Vec<ServiceProfile>> {
        let mut stack = super::lock(&self.stack)?;
        profiles(&mut stack)
    }

    pub fn profile_summary(&self, name: &str) -> Result<Option<ServiceProfileSummary>> {
        let mut stack = super::lock(&self.stack)?;
        let current = current_profile_name(&mut stack)?;
        profile(&mut stack, name).map(|profile| {
            profile.map(|profile| ServiceProfileSummary::from_profile(&profile, current.as_deref()))
        })
    }

    pub fn profile(&self, name: &str) -> Result<Option<ServiceProfile>> {
        validate_profile_name(name)?;
        let mut stack = super::lock(&self.stack)?;
        profile(&mut stack, name)
    }

    pub fn current_profile(&self) -> Result<Option<ServiceProfile>> {
        let mut stack = super::lock(&self.stack)?;
        let Some(name) = current_profile_name(&mut stack)? else {
            return Ok(None);
        };
        profile(&mut stack, &name)
    }

    pub fn select_profile(&self, name: &str) -> Result<ServiceProfileSummary> {
        validate_profile_name(name)?;
        let mut stack = super::lock(&self.stack)?;
        let before = stack.snapshot()?;
        let profile = profile(&mut stack, name)?.ok_or_else(|| Error::ProfileNotFound {
            profile: name.to_owned(),
        })?;
        put_current_profile(&mut stack, name)?;
        self.checkpoint(&mut stack, before)?;
        Ok(ServiceProfileSummary::from_profile(&profile, Some(name)))
    }

    /// Resolve the signing policy for one deferred webhook without storing its
    /// secret in the work envelope or logs.
    pub fn webhook_signing_for(&self, name: Option<&str>) -> Result<Option<WebhookSigning>> {
        let Some(name) = name else {
            return Ok(self.artifact().webhook_policy().signing);
        };
        let Some(profile) = self.profile(name)? else {
            return Ok(self.artifact().webhook_policy().signing);
        };
        let Some(secret) = profile.webhook_signing_secret else {
            return Ok(self.artifact().webhook_policy().signing);
        };
        let inherited = self.artifact().webhook_policy().signing;
        Ok(Some(WebhookSigning {
            secret,
            header: inherited.as_ref().map_or_else(
                || "x-simile-signature".to_owned(),
                |value| value.header.clone(),
            ),
            timestamp_header: inherited.as_ref().map_or_else(
                || "x-simile-timestamp".to_owned(),
                |value| value.timestamp_header.clone(),
            ),
        }))
    }
}

fn resolve_principal_setup(service: &Service, requested: Option<&str>) -> Result<PrincipalSetup> {
    if let Some(principal) = requested {
        return service
            .artifact()
            .principal_setup(principal)
            .ok_or_else(|| Error::PrincipalSetupNotDeclared {
                principal: principal.to_owned(),
            });
    }

    let mut setups = service.artifact().principal_setups();
    let Some(first) = setups.next() else {
        return Err(Error::SetupNotDeclared);
    };
    let Some(second) = setups.next() else {
        return Ok(first);
    };
    let mut principals = vec![first.principal, second.principal];
    principals.extend(setups.map(|setup| setup.principal));
    Err(Error::PrincipalSetupAmbiguous { principals })
}

fn successful_response_status(operation: ArtifactOperation<'_>) -> Option<u16> {
    operation
        .responses()
        .find_map(|response| match response.status() {
            ResponseStatus::Exact(status) if (200..300).contains(status) => Some(*status),
            ResponseStatus::Range(2) => Some(200),
            _ => None,
        })
}

fn profiles(stack: &mut Stack) -> Result<Vec<ServiceProfile>> {
    let result = stack.execute(
        profile_resource()?,
        CommandMetadata::default(),
        StackCommand::Db(DbCommand::Query {
            collection: PROFILES.to_owned(),
            filters: Vec::new(),
            limit: 10_000,
        }),
    )?;
    let StackResult::Db(result) = result.result else {
        return Err(Error::InvalidProfileState);
    };
    let DbResult::Documents(documents) = *result else {
        return Err(Error::InvalidProfileState);
    };
    let mut profiles = documents
        .into_iter()
        .map(|document| decode_profile(document.value))
        .collect::<Result<Vec<_>>>()?;
    profiles.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(profiles)
}

fn build_profile(
    service: &Service,
    name: &str,
    definition: &PrincipalSetup,
    collected: BTreeMap<String, Value>,
    execution: &OperationExecution,
    rng: &mut impl rand::Rng,
) -> Result<ServiceProfile> {
    let response = execution.response().value();
    let mut issued = BTreeMap::new();
    let mut credentials = BTreeMap::new();
    let mut webhook_signing_secret = None;
    for issue in definition.issues.values() {
        let value = match &issue.source {
            SetupIssueSource::Response { pointer } => response
                .and_then(|response| response.pointer(pointer))
                .cloned()
                .ok_or_else(|| Error::SetupResponseValueMissing {
                    issue: issue.name.clone(),
                    pointer: pointer.clone(),
                })?,
            SetupIssueSource::Token { prefix } => {
                Value::String(format!("{prefix}{}", random_hex(rng, 24)))
            }
        };
        if let Some(credential) = &issue.credential {
            let Some(secret) = value.as_str() else {
                return Err(Error::SetupCredentialNotString {
                    issue: issue.name.clone(),
                });
            };
            credentials.insert(
                credential.clone(),
                ProfileCredential::Secret {
                    value: secret.to_owned(),
                },
            );
        }
        if issue.binding == Some(SetupBinding::WebhookSigningSecret) {
            webhook_signing_secret = value.as_str().map(str::to_owned);
        }
        issued.insert(
            issue.name.clone(),
            ProfileValue {
                value,
                sensitive: issue.sensitive,
            },
        );
    }
    let now = service.clock()?.now.as_unix_millis();
    let revision = service.artifact().summary().payload_sha256;
    Ok(ServiceProfile {
        id: format!("profile_{}", random_hex(rng, 12)),
        name: name.to_owned(),
        principal: definition.principal.clone(),
        status: ProfileStatus::Active,
        collected,
        issued,
        credentials,
        webhook_target: None,
        webhook_signing_secret,
        service_revision: revision,
        created_at: now,
        updated_at: now,
    })
}

fn apply_collected_bindings(
    definition: &PrincipalSetup,
    collected: &BTreeMap<String, Value>,
    profile: &mut ServiceProfile,
) -> Result<()> {
    for collect in definition.collects.values() {
        let Some(binding) = collect.binding else {
            continue;
        };
        let Some(value) = collected.get(&collect.name) else {
            continue;
        };
        match binding {
            SetupBinding::WebhookTarget => {
                profile.webhook_target = Some(
                    value
                        .as_str()
                        .ok_or_else(|| Error::InvalidSetupValue {
                            name: collect.name.clone(),
                            expected: "a URL string",
                        })?
                        .to_owned(),
                );
            }
            SetupBinding::WebhookSigningSecret => {
                profile.webhook_signing_secret = Some(
                    value
                        .as_str()
                        .ok_or_else(|| Error::InvalidSetupValue {
                            name: collect.name.clone(),
                            expected: "a string",
                        })?
                        .to_owned(),
                );
            }
        }
    }
    Ok(())
}

fn reject_unknown_values(
    definition: &PrincipalSetup,
    supplied: &BTreeMap<String, Value>,
) -> Result<()> {
    if let Some(name) = supplied
        .keys()
        .find(|name| !definition.collects.contains_key(*name))
    {
        return Err(Error::UnknownSetupValue { name: name.clone() });
    }
    Ok(())
}

fn validate_collected(
    definition: &PrincipalSetup,
    collected: &BTreeMap<String, Value>,
) -> Result<()> {
    for field in definition.collects.values() {
        let Some(value) = collected.get(&field.name) else {
            if field.required {
                return Err(Error::MissingSetupValue {
                    name: field.name.clone(),
                });
            }
            continue;
        };
        let valid = match field.value_type {
            SetupValueType::String => value.is_string(),
            SetupValueType::Url => value
                .as_str()
                .is_some_and(|value| value.starts_with("https://") || value.starts_with("http://")),
            SetupValueType::Boolean => value.is_boolean(),
            SetupValueType::Integer => value.as_i64().is_some() || value.as_u64().is_some(),
            SetupValueType::Json => true,
        };
        if !valid {
            return Err(Error::InvalidSetupValue {
                name: field.name.clone(),
                expected: match field.value_type {
                    SetupValueType::String => "a string",
                    SetupValueType::Url => "an HTTP or HTTPS URL",
                    SetupValueType::Boolean => "a boolean",
                    SetupValueType::Integer => "an integer",
                    SetupValueType::Json => "JSON",
                },
            });
        }
    }
    Ok(())
}

fn set_pointer(root: &mut Value, pointer: &str, value: Value) -> Result<()> {
    let segments = pointer
        .split('/')
        .skip(1)
        .map(|segment| segment.replace("~1", "/").replace("~0", "~"))
        .collect::<Vec<_>>();
    if segments.is_empty() {
        *root = value;
        return Ok(());
    }
    let mut current = root;
    for segment in &segments[..segments.len() - 1] {
        let object = current
            .as_object_mut()
            .ok_or_else(|| Error::InvalidSetupPointer {
                pointer: pointer.to_owned(),
            })?;
        current = object
            .entry(segment.clone())
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
    }
    let object = current
        .as_object_mut()
        .ok_or_else(|| Error::InvalidSetupPointer {
            pointer: pointer.to_owned(),
        })?;
    object.insert(segments.last().cloned().unwrap_or_default(), value);
    Ok(())
}

fn random_hex(rng: &mut impl rand::Rng, length: usize) -> String {
    let mut bytes = vec![0_u8; length];
    rng.fill(bytes.as_mut_slice());
    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut output, byte| {
            write!(output, "{byte:02x}").expect("writing to a String cannot fail");
            output
        },
    )
}

pub(super) fn put_profile(stack: &mut Stack, profile: &ServiceProfile) -> Result<()> {
    stack.execute(
        profile_resource()?,
        CommandMetadata {
            operation_id: Some("simile.setup".to_owned()),
            ..CommandMetadata::default()
        },
        StackCommand::Db(DbCommand::Put {
            collection: PROFILES.to_owned(),
            key: profile.name.clone(),
            value: serde_json::to_value(profile)
                .map_err(|error| Error::ProfileEncoding(error.to_string()))?,
            condition: WriteCondition::Any,
        }),
    )?;
    Ok(())
}

fn profile(stack: &mut Stack, name: &str) -> Result<Option<ServiceProfile>> {
    let receipt = stack.execute(
        profile_resource()?,
        CommandMetadata::default(),
        StackCommand::Db(DbCommand::Get {
            collection: PROFILES.to_owned(),
            key: name.to_owned(),
        }),
    )?;
    let StackResult::Db(result) = receipt.result else {
        return Err(Error::InvalidProfileState);
    };
    let DbResult::Document(document) = *result else {
        return Err(Error::InvalidProfileState);
    };
    document
        .map(|document| decode_profile(document.value))
        .transpose()
}

fn current_profile_name(stack: &mut Stack) -> Result<Option<String>> {
    let receipt = stack.execute(
        profile_resource()?,
        CommandMetadata::default(),
        StackCommand::Db(DbCommand::Get {
            collection: SETTINGS.to_owned(),
            key: CURRENT_PROFILE.to_owned(),
        }),
    )?;
    let StackResult::Db(result) = receipt.result else {
        return Err(Error::InvalidProfileState);
    };
    let DbResult::Document(document) = *result else {
        return Err(Error::InvalidProfileState);
    };
    document
        .map(|document| {
            document
                .value
                .as_str()
                .map(str::to_owned)
                .ok_or(Error::InvalidProfileState)
        })
        .transpose()
}

fn put_current_profile(stack: &mut Stack, name: &str) -> Result<()> {
    stack.execute(
        profile_resource()?,
        CommandMetadata {
            operation_id: Some("simile.use".to_owned()),
            ..CommandMetadata::default()
        },
        StackCommand::Db(DbCommand::Put {
            collection: SETTINGS.to_owned(),
            key: CURRENT_PROFILE.to_owned(),
            value: Value::String(name.to_owned()),
            condition: WriteCondition::Any,
        }),
    )?;
    Ok(())
}

fn decode_profile(value: Value) -> Result<ServiceProfile> {
    serde_json::from_value(value).map_err(|error| Error::ProfileEncoding(error.to_string()))
}

fn profile_resource() -> Result<ResourceAddress> {
    Ok(ResourceAddress::new(
        simile_spec::ResourceKind::Db,
        PROFILE_STATE,
    )?)
}

pub fn validate_profile_name(name: &str) -> Result<()> {
    let mut characters = name.chars();
    let valid_start = characters
        .next()
        .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit());
    let valid_rest = characters.all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '.' | '_' | '-')
    });
    if valid_start && valid_rest {
        Ok(())
    } else {
        Err(Error::InvalidProfileName {
            profile: name.to_owned(),
        })
    }
}
