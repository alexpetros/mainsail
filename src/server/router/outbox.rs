use serde_json::json;
use crate::activitypub::objects::actor::Actor;
use crate::server::ap_request::UnverifiedApRequest;
use crate::server::response::ServerResult;
use crate::local::actors::{get_outbox, get_outbox_page};
use crate::server::response;
use crate::utils::get_single_query_value;

pub async fn get(req: UnverifiedApRequest, actor: Actor) -> ServerResult {
    let query = req.uri().query();
    let page_num = query
        .and_then(|q| get_single_query_value(q, "page"))
        .and_then(|p| { p.parse::<usize>().ok() });

    if let Some(p) = page_num {
        let outbox_page = get_outbox_page(&req.db, &actor.id, p)?;
        let body = json!(outbox_page).to_string();
        response::send(body)
    } else {
        let outbox = get_outbox(&req.db, &actor.id)?;
        let body = json!(outbox).to_string();
        response::send(body)
    }
}
