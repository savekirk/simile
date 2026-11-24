//! Attach a selected local profile using the artifact's actual security schemes.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use http::{HeaderName, HeaderValue, Request};
use simile_service::Service;
use simile_spec::{ParameterLocation, ProfileCredential, SecurityScheme};

#[derive(Debug, thiserror::Error)]
pub enum ProfileRequestError {
    #[error("profile could not be loaded")]
    Profile,
    #[error("profile credential cannot be represented by this HTTP scheme")]
    Credential,
    #[error("profile credential cannot be represented in the request head")]
    Request,
}

pub fn apply_profile<T>(
    service: &Service,
    name: &str,
    request: &mut Request<T>,
) -> Result<(), ProfileRequestError> {
    let profile = service
        .profile_snapshot(name)
        .map_err(|_| ProfileRequestError::Profile)?
        .ok_or(ProfileRequestError::Profile)?;
    let mut headers = request.headers().clone();
    // Keep unrelated wire fields byte-for-byte, including repeated cookie headers.
    let mut query: Vec<String> = request
        .uri()
        .query()
        .unwrap_or("")
        .split('&')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect();
    let mut query_changed = false;
    for (name, credential) in &profile.credentials {
        let scheme = service
            .artifact()
            .security_scheme(name)
            .ok_or(ProfileRequestError::Credential)?
            .scheme();
        let (location, name, value) = match (scheme, credential) {
            (SecurityScheme::ApiKey { location, name }, ProfileCredential::Secret { value }) => {
                (location, name, value.clone())
            }
            (
                SecurityScheme::Http { scheme, .. },
                ProfileCredential::Basic { username, password },
            ) if scheme.eq_ignore_ascii_case("basic") => (
                ParameterLocation::Header,
                "authorization".to_owned(),
                format!(
                    "Basic {}",
                    STANDARD.encode(format!("{username}:{password}"))
                ),
            ),
            (SecurityScheme::Http { scheme, .. }, ProfileCredential::Secret { value }) => (
                ParameterLocation::Header,
                "authorization".to_owned(),
                format!("{scheme} {value}"),
            ),
            (
                SecurityScheme::OAuth2 | SecurityScheme::OpenIdConnect,
                ProfileCredential::Secret { value },
            ) => (
                ParameterLocation::Header,
                "authorization".to_owned(),
                format!("Bearer {value}"),
            ),
            _ => return Err(ProfileRequestError::Credential),
        };
        match location {
            ParameterLocation::Header => {
                headers.insert(
                    HeaderName::try_from(name).map_err(|_| ProfileRequestError::Request)?,
                    HeaderValue::try_from(value).map_err(|_| ProfileRequestError::Request)?,
                );
            }
            ParameterLocation::Query => {
                query.retain(|pair| {
                    form_urlencoded::parse(pair.as_bytes())
                        .next()
                        .is_none_or(|(key, _)| key != name)
                });
                query.push(
                    form_urlencoded::Serializer::new(String::new())
                        .append_pair(&name, &value)
                        .finish(),
                );
                query_changed = true;
            }
            ParameterLocation::Cookie => {
                if value.contains([';', '\r', '\n']) {
                    return Err(ProfileRequestError::Request);
                }
                let existing: Vec<HeaderValue> = headers
                    .get_all(http::header::COOKIE)
                    .iter()
                    .cloned()
                    .collect();
                headers.remove(http::header::COOKIE);
                for cookie in existing {
                    let text = cookie.to_str().map_err(|_| ProfileRequestError::Request)?;
                    let parts: Vec<_> = text
                        .split(';')
                        .filter(|part| {
                            part.trim()
                                .split_once('=')
                                .is_none_or(|(key, _)| key != name)
                        })
                        .collect();
                    if parts.len() == text.split(';').count() {
                        headers.append(http::header::COOKIE, cookie);
                    } else if !parts.is_empty() {
                        headers.append(
                            http::header::COOKIE,
                            HeaderValue::try_from(parts.join(";"))
                                .map_err(|_| ProfileRequestError::Request)?,
                        );
                    }
                }
                headers.append(
                    http::header::COOKIE,
                    HeaderValue::try_from(format!("{name}={value}"))
                        .map_err(|_| ProfileRequestError::Request)?,
                );
            }
            ParameterLocation::Path => return Err(ProfileRequestError::Credential),
        }
    }
    if query_changed {
        let query = query.join("&");
        let mut parts = request.uri().clone().into_parts();
        parts.path_and_query = Some(
            format!("{}?{query}", request.uri().path())
                .parse()
                .map_err(|_| ProfileRequestError::Request)?,
        );
        *request.uri_mut() =
            http::Uri::from_parts(parts).map_err(|_| ProfileRequestError::Request)?;
    }
    *request.headers_mut() = headers;
    Ok(())
}
