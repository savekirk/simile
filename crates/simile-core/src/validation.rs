use crate::{Error, Result};

pub(crate) fn validate_service_name(name: &str) -> Result<()> {
    let mut characters = name.chars();
    let starts_safely = characters
        .next()
        .is_some_and(|character| character.is_ascii_lowercase() || character.is_ascii_digit());
    let rest_is_safe = characters.all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '.' | '_' | '-')
    });
    if starts_safely && rest_is_safe && name.len() <= 64 {
        Ok(())
    } else {
        Err(Error::InvalidRegisteredServiceName {
            name: name.to_owned(),
        })
    }
}

pub(crate) fn validate_endpoint(endpoint: &str) -> Result<()> {
    let endpoint = endpoint.trim();
    let uri = endpoint.parse::<simile_http::http::Uri>();
    let valid = uri.as_ref().is_ok_and(|uri| {
        let Some(authority) = uri.authority() else {
            return false;
        };
        let secure = uri.scheme_str() == Some("https");
        let local = uri.scheme_str() == Some("http")
            && matches!(
                authority.host(),
                "localhost" | "127.0.0.1" | "[::1]" | "::1"
            );
        (secure || local)
            && uri.path().starts_with('/')
            && uri.path() != "/"
            && uri.query().is_none()
    });
    if valid && !endpoint.ends_with('/') {
        Ok(())
    } else {
        Err(Error::InvalidReportingEndpoint {
            endpoint: endpoint.to_owned(),
        })
    }
}
