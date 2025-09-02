use url::Url;
use reqwest::Response;
use http::{HeaderValue, Method, StatusCode};
use http::header::{ACCEPT, DATE, USER_AGENT};
use serde::de::DeserializeOwned;
use crate::activitypub::objects::activities::Activity;
use crate::activitypub::signature::{get_ap_date, sign};
use crate::activitypub::webfinger::WebFinger;
use crate::database::Database;
use crate::error::{bad_gateway, MainsailError};
use crate::local::actors::LocalActorStub;
use crate::local::InternalResult;
use crate::utils;

/// Gateway - the layer that retrieves activitypub content from other servers
///
/// This is in its own top-level directory because it needs to be accessible across the package
/// but not get exported.
///
/// This is also going to be the caching layer. Requests come to the gateway, and the gateway
/// determines which of those requests can be fulfilled by the cache, and which ones require a
/// network hit.
pub mod federation;

static SHORT_ACCEPT_HEADER: HeaderValue = HeaderValue::from_static("application/activity+json");


async fn send_ap_req(url: &Url, sender: &LocalActorStub, body: Option<String>) -> InternalResult<Response> {
    let date = get_ap_date();
    let client = reqwest::Client::new();

    let method = if body.is_some() { Method::POST } else { Method::GET };
    let header = HeaderValue::from_str(&date).map_err(|_| { MainsailError::InternalInvalidHeader })?;

    let mut request = client
        .request(method.clone(), url.as_str())
        .header(DATE, header)
        .header(USER_AGENT, "Sailboat/0.0.1")
        .header(ACCEPT, &SHORT_ACCEPT_HEADER);

    if let Some(body) = body { request = request.body(body); }

    let request = request.build().map_err(MainsailError::InternalInvalidRequest)?;
    let request = sign(request, sender)?;

    client.execute(request).await.map_err(MainsailError::FailedToContactActor)
}

pub async fn get_from_ap<T>(url: &Url, sender: &LocalActorStub) -> InternalResult<T>
where
    T: DeserializeOwned,
{
    let res = send_ap_req(url, sender, None).await?;
    if res.status() == StatusCode::GONE {
        return Err(bad_gateway("Requested a URL that no longer exists"));
    }

    let body = res.text().await.map_err(MainsailError::InvalidActorBody)?;
    let item: T = utils::deserialize_json(&body)?;
    Ok(item)
}

pub async fn post_to_ap(url: &Url, sender: &LocalActorStub, body: String) -> InternalResult<Response>
{
    let res = send_ap_req(url, sender, Some(body)).await;

    if let Ok(res) = res.as_ref()
        && res.status().as_u16() > 399 {
            tracing::warn!("Warning, received {} from {}", res.status(), &url);
        }

    res
}

pub async fn send_activity(db: Database, url: Url, sender: LocalActorStub, activity: &impl Activity) -> InternalResult<()> {
    let id = activity.get_id();
    let body = activity.json();

    db.execute("INSERT INTO sent_activities (activity_id, actor_id, object) VALUES (?1, ?2, ?3)",
               (id.as_str(), sender.id.clone(), &body))?;

    let res = post_to_ap(&url, &sender, body).await?.error_for_status();
    match res {
        Err(e) => {
            tracing::warn!("Failed to send activity {} to {}: {:?}", id, url, e);
            Err(MainsailError::InternalServerError(e.to_string()))
        }
        Ok(_) => Ok(())
    }
}

pub async fn get_webfinger(host: &str, account_name: &str) -> InternalResult<WebFinger> {
    let url = format!("https://{host}/.well-known/webfinger");
    let resource = format!("acct:{account_name}@{host}");
    let request = reqwest::Client::new()
        .get(&url)
        .header(USER_AGENT, "Sailboat/0.0.1")
        .header(ACCEPT, &SHORT_ACCEPT_HEADER)
        .query(&[("resource", resource)]);
    let res = request.send().await.map_err(MainsailError::FailedToContactActor)?;
    let text = res.text().await.map_err(MainsailError::InvalidActorBody)?;

    let web_finger: WebFinger = utils::deserialize_json(&text)?;
    Ok(web_finger)
}