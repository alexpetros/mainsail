//! ActivityPub [Accept Activity](https://www.w3.org/TR/activitystreams-vocabulary/#dfn-accept)
use serde::{Deserialize, Serialize};
use url::Url;
use crate::activitypub::{AtContext, Context};
use crate::activitypub::objects::activities::follow::FollowActivity;
use crate::server::ap_request::AuthedApRequest;
use crate::local::actors::LocalActorStub;
use crate::server::response::ServerResult;
use crate::database::Database;
use crate::gateway;
use crate::local::InternalResult;
use crate::server::response;
use crate::utils::new_activity_id;

use super::Activity;

#[derive(Debug, Serialize, Deserialize)]
enum AcceptActivityType { Accept }

#[derive(Debug, Serialize, Deserialize)]
pub struct AcceptActivity<A> {
    #[serde(rename = "type")]
    activity_type: AcceptActivityType,
    #[serde(rename = "@context")]
    pub context: AtContext,
    pub id: Url,
    pub actor: String,
    pub object: A,
}

impl<A: Serialize> AcceptActivity<A> {
    pub fn new(domain: &str, actor_id: String, activity: A) -> Self {
        AcceptActivity {
            context: AtContext::Context(Context::ActivityStreams),
            activity_type: AcceptActivityType::Accept,
            id: new_activity_id(domain).unwrap(),
            actor: actor_id,
            object: activity,
        }
    }
}

impl<A: Serialize> Activity for AcceptActivity<A> {
    fn get_id(&self) -> &Url {
        &self.id
    }

    fn json(&self) -> String {
        serde_json::json!(self).to_string()
    }
}

#[tracing::instrument(name = "send_accept", skip(activity, sender), fields(
    inbox_url = % inbox_url.as_str()
))]
pub async fn send(db: Database, sender: LocalActorStub, inbox_url: Url, activity: &impl Activity) -> InternalResult<()> {
    tracing::info!("Sending accept for activity: {}", activity.get_id());
    let accept_activity = AcceptActivity::new(&sender.domain, sender.id.clone(), activity);
    gateway::send_activity(db, inbox_url, sender, &accept_activity).await.inspect_err(|e| {
        tracing::error!("Failed to send activity: {}", e)
    })
}

// Right now this only accepts follow requests lol
pub fn receive(req: AuthedApRequest, accept_activity: AcceptActivity<serde_json::Value>) -> ServerResult {
    tracing::debug!("Accepting {} from {}", &accept_activity.id, &req.actor.id);
    let follow_activity = serde_json::from_value::<FollowActivity>(accept_activity.object);

    let Ok(follow_activity) = follow_activity else {
        return response::not_implemented()
    };

    let changes = req.db.execute(
        "UPDATE following SET is_pending = FALSE, accept_activity = ?1 WHERE activity_id = ?2",
        (&accept_activity.id.as_str(), &follow_activity.id.as_str()),
    )?;

    if changes == 0 {
        tracing::warn!("AcceptActivity sent for unknown follow to {} with id {}",
            follow_activity.actor, follow_activity.object);
        response::not_found()
    } else {
        response::ok()
    }
}

#[cfg(test)]
mod tests {
    use http::StatusCode;

    use url::Url;
    use crate::activitypub::objects::activities::follow::FollowActivity;
    use crate::activitypub::objects::activities::Activity;
    use crate::activitypub::AtContext;
    use crate::activitypub::Context;
    use crate::local::actors::LocalActorStub;
    use crate::database::Database;

    use crate::tests::data::EXT_ACTOR_1;
    use crate::tests::get_memory_db;
    use crate::tests::get_named_memory_db;
    use crate::tests::requests::create_authed_inbox_post;
    use crate::tests::test_server::TestServer;

    use super::AcceptActivity;
    use super::AcceptActivityType;

    const ACTOR_ID: &str = "https://example.com/_mainsail/actors/1";
    const ACCEPT_ACTIVITY_ID: &str = "https://other.example.com/activity/1";
    const UNKNOWN_FOLLOW_ACTIVITY: &str = "https://example.com/activity/bad";

    #[tokio::test]
    async fn send_accept_follow() {
        let (db, name) = get_memory_db();
        let sender = LocalActorStub::get(&db, ACTOR_ID).unwrap();

        let mut test_server = TestServer::new(&name).await;
        let actor_who_followed = test_server.new_actor();
        test_server.set_inbox_response(&actor_who_followed, 200).await;

        let inbox_url: Url = actor_who_followed.inbox.parse().unwrap();
        let follow_activity = FollowActivity::new_internal(&sender, &actor_who_followed.id);
        let req_db = get_named_memory_db(&name);
        let sender_id = sender.id.clone();
        super::send(req_db, sender, inbox_url, &follow_activity).await.unwrap();

        let requests = test_server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);

        let accept_activity: AcceptActivity<FollowActivity> = requests[0].body_json().unwrap();
        assert_eq!(accept_activity.actor, sender_id);
        assert_eq!(accept_activity.object.object, actor_who_followed.id);
    }

    #[test]
    fn receive_accept_follow() {
        let (db, name) = get_memory_db();
        let sender = LocalActorStub::get(&db, ACTOR_ID).unwrap();

        let follow_activity = FollowActivity::new_internal(&sender, &EXT_ACTOR_1.id);
        let follow_activity_id = follow_activity.id.to_owned();
        let accept_activity = mock_accept_follow(follow_activity);

        make_pending_follow(&db, follow_activity_id.as_str(), ACTOR_ID, &EXT_ACTOR_1.id);

        let req = create_authed_inbox_post(accept_activity.json(), &name, &EXT_ACTOR_1);
        let res = super::receive(req, accept_activity).unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(num_following_accepted(&db), 1);
        assert_eq!(num_following_pending(&db), 0);
    }

    #[test]
    fn receive_accept_follow_not_found() {
        let (db, name) = get_memory_db();
        let sender = LocalActorStub::get(&db, ACTOR_ID).unwrap();

        let follow_activity = FollowActivity::new_internal(&sender, &EXT_ACTOR_1.id);
        let accept_activity = mock_accept_follow(follow_activity);

        make_pending_follow(&db, UNKNOWN_FOLLOW_ACTIVITY, ACTOR_ID, &EXT_ACTOR_1.id);

        let req = create_authed_inbox_post(accept_activity.json(), &name, &EXT_ACTOR_1);
        let res = super::receive(req, accept_activity).unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(num_following_accepted(&db), 0);
        assert_eq!(num_following_pending(&db), 1);
    }

    #[test]
    fn receive_accept_unknown_activity() {
        let (_db, name) = get_memory_db();
        let accept_activity = mock_accept_unknown();

        let req = create_authed_inbox_post(accept_activity.json(), &name, &EXT_ACTOR_1);
        let res = super::receive(req, accept_activity).unwrap();

        assert_eq!(res.status(), StatusCode::NOT_IMPLEMENTED);
    }

    fn make_pending_follow(db: &Database, id: &str, actor_id: &str, followed_actor: &str) {
        db.execute(
            "INSERT OR REPLACE INTO following (activity_id, actor_id, followed_actor_id)
            VALUES (?1, ?2, ?3)",
            (&id, actor_id, followed_actor),
        ).unwrap();
    }

    fn mock_accept_follow(follow_activity: FollowActivity) -> AcceptActivity<serde_json::Value> {
        AcceptActivity {
            context: AtContext::Context(Context::ActivityStreams),
            activity_type: AcceptActivityType::Accept,
            id: ACCEPT_ACTIVITY_ID.parse().unwrap(),
            actor: EXT_ACTOR_1.id.to_owned(),
            object: serde_json::json!(follow_activity),
        }
    }

    fn mock_accept_unknown() -> AcceptActivity<serde_json::Value> {
        AcceptActivity {
            context: AtContext::Context(Context::ActivityStreams),
            activity_type: AcceptActivityType::Accept,
            id: ACCEPT_ACTIVITY_ID.parse().unwrap(),
            actor: EXT_ACTOR_1.id.to_owned(),
            object: serde_json::json!(""),
        }
    }

    fn num_following_accepted(db: &Database) -> i64 {
        db.query_row(
            "SELECT count(actor_id) FROM following WHERE actor_id = ?1 AND is_pending = FALSE",
            (ACTOR_ID,),
            |r| { r.get(0) },
        ).unwrap()
    }

    fn num_following_pending(db: &Database) -> i64 {
        db.query_row(
            "SELECT count(actor_id) FROM following WHERE actor_id = ?1 AND is_pending = TRUE",
            (ACTOR_ID,),
            |r| { r.get(0) },
        ).unwrap()
    }
}
