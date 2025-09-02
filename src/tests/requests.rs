use hyper::body::Bytes;
use crate::activitypub::objects::activities::GenericActivity;
use crate::activitypub::objects::actor::Actor;
use crate::server::ap_request::{AuthedApRequest, UnverifiedApRequest};
use crate::tests;

pub fn create_unverified_get(url: &str, name: &str) -> UnverifiedApRequest {
    let request = hyper::Request::builder()
        .method("GET")
        .uri(url)
        .header("Content-Type", "activity/json")
        .body(Bytes::new())
        .unwrap();

    let db = tests::get_named_memory_db(name);
    let domain = "example.com".to_string();

    UnverifiedApRequest { request, db, domain, actor: crate::server::ap_request::NoAuth }
}

pub fn create_authed_post(url: &str, body_content: &str, name: &str, sender: &Actor) -> AuthedApRequest {
    let body = Bytes::copy_from_slice(body_content.as_bytes());
    let request = hyper::Request::builder()
        .method("POST")
        .uri(url)
        .header("Accept", "activity/json")
        .body(body)
        .unwrap();

    let db = tests::get_named_memory_db(name);
    let domain = "example.com".to_string();

    AuthedApRequest { request, db, domain, actor: sender.clone() }
}

pub fn create_authed_inbox_post(body_content: String, db_name: &str, sender: &Actor) -> AuthedApRequest {
    let req = create_authed_post("/inbox", &body_content, db_name, sender);

    let activity: GenericActivity = serde_json::from_slice(body_content.as_bytes()).unwrap();
    let mut stmt = req.db.prepare("INSERT OR IGNORE INTO received_activities (activity_id, actor_id, object) VALUES (?1, ?2, ?3)").unwrap();

    stmt.execute([activity.id.as_str(), &activity.actor, &body_content]).unwrap();
    drop(stmt);

    req
}