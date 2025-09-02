use serde_json::json;
use http::header::{CONTENT_TYPE, HeaderValue};
use crate::server::ap_request::UnverifiedApRequest;
use crate::local::actors;
use crate::server::response::{MainsailResponse, ServerResult};

pub async fn get_profile(req: UnverifiedApRequest) -> ServerResult {
    let profile_param = req.get_trailing_param("Missing profile ID")?;

    let uuid = match profile_param.split_once('#') {
        None => profile_param,
        Some((f, _)) => f,
    };

    let actor = actors::get_actor_by_uuid(&req.db, uuid)?;
    let body = json!(actor).to_string();
    let header_value = HeaderValue::from_static("application/activity+json");
    let mut res = MainsailResponse::new_with_body(body);
    res.set_header(CONTENT_TYPE, header_value);
    Ok(res)
}

#[cfg(test)]
mod tests {
    use http::StatusCode;

    use crate::activitypub::objects::actor::Actor;
    use crate::tests::requests::create_unverified_get;
    use crate::tests::get_memory_db;

    const ACTOR_ID: &str = "https://example.com/_mainsail/actors/1";

    #[tokio::test]
    async fn get_profile() {
        let (_db, name) = get_memory_db();
        let uri = "/profiles/1";
        let req = create_unverified_get(uri, &name);
        let res = super::get_profile(req).await.unwrap();

        let body = res.into_body_string().await;
        let actor: Actor = serde_json::from_str(&body).unwrap();

        assert_eq!(actor.id, ACTOR_ID);
    }

    #[tokio::test]
    async fn get_profile_not_found() {
        let (_db, name) = get_memory_db();
        let uri = "/profiles/2";
        let req = create_unverified_get(uri, &name);
        let res = super::get_profile(req).await.unwrap_err();

        assert_eq!(res.status(), StatusCode::NOT_FOUND)
    }
}
