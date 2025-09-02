use crate::activitypub::objects::actor::LinkType;
use crate::activitypub::webfinger::WebFinger;
use crate::activitypub::webfinger::WebFingerLink;
use crate::error;
use crate::local::actors;
use crate::local::actors::get_server_actor;
use crate::error::bad_request;
use crate::server::response::ServerResult;
use crate::utils::get_single_query_value;
use serde_json::json;
use tracing::debug;
use tracing::warn;
use crate::server::ap_request::UnverifiedApRequest;
use crate::server::response;

pub async fn get(req: UnverifiedApRequest) -> ServerResult {
    let query = req
        .uri()
        .query()
        .ok_or(bad_request("Missing query parameter"))?;

    let resource = get_single_query_value(query, "resource")
        .ok_or(bad_request("Invalid query string provided"))?;

    let decoded_resource = urlencoding::decode(resource).map_err(|_| {
        bad_request("Query string could not be decoded")
    })?;

    let (search_type, identifier) = decoded_resource.split_once(':').ok_or_else(|| {
        bad_request("Invalid resource query provided (missing scheme i.e. 'acct:')")
    })?;

    if search_type != "acct" {
        warn!("Received search type: {}", search_type);
        return Err(bad_request(
            "Sorry, that scheme is not supported yet (expected 'acct:')",
        ));
    }

    // TODO check the domain
    let (preferred_username, domain) = identifier
        .split_once('@')
        .ok_or_else(|| bad_request("Invalid handle resource provided"))?;

    debug!("Searching for user {} {}", preferred_username, domain);

    let actor = if preferred_username == "MainsailInternal" {
        get_server_actor(&req.db)?
    } else {
        actors::get_actor_by_preferred_username(&req.db, preferred_username)
            .ok_or(error::not_found())?
    };

    let self_link = WebFingerLink {
        rel: "self".to_owned(),
        link_type: Some(LinkType::ActivityJson),
        href: Some(actor.id)
    };

    let links = vec![self_link];
    let actor = WebFinger {
        subject: Some(format!("acct:{preferred_username}@{domain}")),
        aliases: None,
        properties: None,
        links: Some(links),
    };

    let body = json!(actor).to_string();
    response::send(body)
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};
    use crate::tests::get_memory_db;
    use crate::tests::requests::create_unverified_get;
    use crate::utils::deserialize_json;

    #[tokio::test]
    async fn find_user_missing_query_param() {
        let (_db, name) = get_memory_db();
        let req = create_unverified_get("/.well-known/webfinger", &name);
        let res = super::get(req).await.unwrap_err();
        assert_eq!(res.status(), 400);
    }

    #[tokio::test]
    async fn find_user_missing_invalid_resource() {
        let (_db, name) = get_memory_db();
        let req = create_unverified_get("/.well-known/webfinger?resource=alex@example.com", &name);
        let res = super::get(req).await.unwrap_err();
        assert_eq!(res.status(), 400);
    }

    #[tokio::test]
    async fn find_user_missing_unknown_user() {
        let (_db, name) = get_memory_db();
        let req = create_unverified_get("/.well-known/webfinger?resource=acct:alex@example.com", &name);
        let res = super::get(req).await.unwrap_err();
        assert_eq!(res.status(), 404);
    }

    #[tokio::test]
    async fn find_user_missing_known_user() {
        let (_db, name) = get_memory_db();
        let req = create_unverified_get("/.well-known/webfinger?resource=acct:awp@example.com", &name);
        let res = super::get(req).await.unwrap();
        assert_eq!(res.status(), 200);

        let expected_json = json!({
            "aliases": null,
            "links": [
                {
                  "href": "https://example.com/_mainsail/actors/1",
                  "rel": "self",
                  "type": "application/activity+json"
                 }
             ],
            "properties": null,
            "subject": "acct:awp@example.com"
        });

        let body = res.into_body_string().await;
        let body_json: Value = deserialize_json(&body).unwrap();
        assert_eq!(body_json, expected_json);
    }
}
