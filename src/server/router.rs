use http::{HeaderValue, Request};
use hyper::body::Bytes;
use http::header::CONTENT_TYPE;
use crate::activitypub::objects::actor::Actor;
use crate::database::Database;
use crate::server::ap_request::new_ap_request;
use crate::local::actors::{self, get_server_actor};
use crate::server::{GET, POST, response};
use crate::server::response::{MainsailResponse, ServerResult};

mod notes;
mod inbox;
mod outbox;
mod profiles;
mod webfinger;

#[rustfmt::skip]
// #[tracing::instrument(skip(req, db))]
pub async fn router(req: Request<Bytes>, db: Database) -> ServerResult {
    // Set up the domain from either the global context or the database
    let domain = db.query_row("SELECT value FROM globals WHERE key = 'domain'", (), |row| row.get(0))?;
    let req = new_ap_request(req, db, domain).await?;
    let path = req.uri().path();

    // Split into sub-routes
    let mut sub_routes: Vec<&str> = path.split('/').collect();
    if let Some(x) = sub_routes.last() {
        // If there's a trailing slash, drop it
        if x.is_empty() { sub_routes.pop(); }
    }
    let sub_routes = &sub_routes[1..];

    // Look for the non-handle routes
    match (req.method(), sub_routes) {
        (POST, ["inbox"]) => return inbox::post(req).await,
        (GET, ["profiles", _]) => return profiles::get_profile(req).await, // legacy
        (GET, ["notes", ..]) => return notes::get(req).await,
        (GET, [".well-known", "webfinger"]) => return webfinger::get(req).await,
        (GET, ["_mainsail", "actors", _]) => return profiles::get_profile(req).await, // legacy?
        (GET, ["_mainsail", "server-actor"]) => {
            let actor = get_server_actor(&req.db)?;
            return serve_actor_json(actor)
        }
        _ => ()
    }

    // Finally, try to match the sub-route with the actor
    let Some(handle) = sub_routes.first() else { return response::not_found() };
    if !handle.starts_with('@') { return response::not_found() };
    let preferred_username = &handle[1..];
    let actor = actors::get_actor_by_preferred_username(&req.db, preferred_username);
    let Some(actor) = actor else {
        return response::not_found()
    };

    match (req.method(), sub_routes) {
        (GET, [_]) => serve_actor_json(actor),
        (GET, [_, "outbox"]) => outbox::get(req, actor).await,
        (POST, [_, "inbox"]) => inbox::post(req).await,
        (GET, [_, "followers"]) => actors::get_followers(req, actor).await,
        (GET, [_, "following"]) => actors::get_following(req, actor).await,
        _ => response::not_found()
    }
}

fn serve_actor_json(actor: Actor) -> ServerResult {
    let body = actor.json();
    let header_value = HeaderValue::from_static("application/activity+json");
    let mut res = MainsailResponse::new_with_body(body);
    res.set_header(CONTENT_TYPE, header_value);
    Ok(res)
}
