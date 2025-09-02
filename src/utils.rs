use chrono::Utc;
use tracing::warn;
use uuid::Uuid;
use crate::error::MainsailError;
use serde::Deserialize;
use url::Url;
use crate::local::InternalResult;


#[derive(Debug, Deserialize)]
struct ApErrorJson {
    error: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ApJson<T> {
    Object(T),
    Error(ApErrorJson),
}


pub fn deserialize_json<'a, T>(text: &'a str) -> Result<T, MainsailError>
where
    T: Deserialize<'a>,
{
    let res = serde_json::from_str::<ApJson<T>>(text).map_err(|e| {
        warn!("Failed to deserialize response: \"{}\" Error: {}", text, e);
        MainsailError::InvalidActivityPubJson(text.into())
    })?;

    match res {
        ApJson::Object(o) => Ok(o),
        ApJson::Error(e) => Err(MainsailError::ActivityPubError(e.error))
    }
}

pub fn new_activity_id(domain: &str) -> InternalResult<Url> {
    let id = Uuid::new_v4().to_string();
    let url = format!("https://{domain}/activity/{id}");
    url.parse().map_err(|_| {
        let message = format!("Failed to parse activity_id {url}");
        MainsailError::InternalServerError(message)
    })
}

pub fn now_formatted() -> String {
    let now = Utc::now();
    // 2025-04-23T23:26:53Z
    now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true).to_string()
}

/// Get the value of a query parameter when that query only has one parameter
pub fn get_single_query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    let split = query.split_once("=")?;
    if key == split.0 {
        Some(split.1)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::get_single_query_value;

    #[test]
    fn single_param() {
        let value = get_single_query_value("page=1", "page").unwrap();
        assert_eq!(value, "1");
    }

    #[test]
    fn invalid_query() {
        let value = get_single_query_value("???", "page");
        assert!(value.is_none())
    }

    #[test]
    fn missing_value() {
        let value = get_single_query_value("resource=1", "page");
        assert!(value.is_none())
    }
}
