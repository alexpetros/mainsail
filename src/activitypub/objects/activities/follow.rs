//! ActivityPub [Follow Activity](https://www.w3.org/TR/activitystreams-vocabulary/#dfn-follow)
use serde::{Deserialize, Serialize};
use url::Url;
use crate::external::get_external_actor;
use crate::activitypub::{AtContext, Context};
use crate::activitypub::objects::activities::undo::UndoActivity;
use crate::activitypub::ApUrl;
use crate::server::ap_request::AuthedApRequest;
use crate::gateway::send_activity;
use crate::local::actors::LocalActorStub;
use crate::local::notifications::insert_follow_notification;
use crate::error::{self, bad_gateway, bad_request, failed_deserialization};
use crate::server::response::{self};
use crate::server::response::ServerResult;
use crate::database::Database;
use crate::local::InternalResult;
use crate::utils::new_activity_id;

use super::{accept, Activity, undo};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum FollowActivityType { Follow }

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FollowActivity {
    #[serde(rename = "type")]
    activity_type: FollowActivityType,
    #[serde(rename = "@context")]
    pub context: Option<AtContext>,
    pub id: Url,
    pub actor: ApUrl, // The actor doing the following
    pub object: ApUrl, // The actor being followed
}

impl FollowActivity {
    pub fn new_internal(actor: &LocalActorStub, actor_to_follow: &str) -> Self {
        FollowActivity {
            context: Some(AtContext::Context(Context::ActivityStreams)),
            activity_type: FollowActivityType::Follow,
            id: new_activity_id(&actor.domain).unwrap(),
            actor: actor.id.to_owned(),
            object: actor_to_follow.to_owned(),
        }
    }
}

impl Activity for FollowActivity {
    fn get_id(&self) -> &Url {
        &self.id
    }

    fn json(&self) -> String {
        serde_json::json!(self).to_string()
    }
}

#[tracing::instrument(skip_all)]
pub async fn send(db: Database, sender: LocalActorStub, actor_to_follow: &str, actor_url: Url) -> InternalResult<()> {
    let follow_activity = FollowActivity::new_internal(&sender, actor_to_follow);
    // This saves a follow as pending until it's accepted with an AcceptActivity
    db.execute(
        "INSERT OR REPLACE INTO following (actor_id, followed_actor_id, activity_id) VALUES (?1, ?2, ?3)",
        (&sender.id, actor_to_follow, follow_activity.id.as_str()))?;
    send_activity(db, actor_url, sender, &follow_activity).await?;
    Ok(())
}

#[tracing::instrument(name = "follow", skip_all)]
pub async fn receive(req: AuthedApRequest, follow_activity: FollowActivity) -> ServerResult {
    let actor_id = &follow_activity.object;

    let actor = LocalActorStub::get(&req.db, actor_id).ok_or(error::not_found())?;
    let sender = get_external_actor(req.db.clone_conn()?, &actor, &follow_activity.actor).await?;

    let inbox_url = sender.inbox.parse()
        .map_err(|_| {
            let message = format!("{} is not a valid inbox URI", sender.inbox);
            bad_gateway(&message)
        })?;

    req.db.execute(
        "INSERT OR REPLACE INTO followers (activity_id, actor_id, sender_id) VALUES (?1, ?2, ?3)",
        (follow_activity.id.as_str(), &actor_id, sender.id))?;
    insert_follow_notification(&req.db, actor_id, &follow_activity)?;

    tokio::spawn(async move {
        let _ = accept::send(req.db, actor, inbox_url, &follow_activity).await;
    });

    response::ok()
}

pub async fn undo_send(db: Database, actor: LocalActorStub, followed_actor_id: &str) -> InternalResult<()> {
    let follow_activity_object: String = db.query_row("
        SELECT sent_activities.object
        FROM sent_activities
        LEFT JOIN following USING (activity_id)
        WHERE following.actor_id = ?1
        ",
        [&actor.id],
        |row| { row.get(0) })?;
    let follow_activity: FollowActivity = serde_json::from_str(&follow_activity_object)
        .map_err(|_| failed_deserialization(follow_activity_object))?;

    let new_db = db.clone_conn()?;
    let followed_actor = get_external_actor(new_db, &actor, followed_actor_id).await?;

    db.execute("DELETE FROM following WHERE followed_actor_id = ?", [&followed_actor_id])?;
    let inbox_url = followed_actor.inbox.parse().map_err(|_| bad_request("Invalid actor URI provided"))?;
    undo::send(db, actor, inbox_url, &follow_activity).await
}

pub fn undo_receive(req: AuthedApRequest, undo_activity: UndoActivity<FollowActivity>) -> ServerResult {
    let id_to_unfollow = &undo_activity.object.object;

    // This is slightly different than what I'm doing for the other ones
    // Is it possible for an actor to follow me on behalf of someone else?
    // This assumes no
    if undo_activity.actor != undo_activity.object.actor {
        return response::not_found();
    }

    let res = req.db.execute("DELETE FROM followers WHERE actor_id = ?1 AND sender_id = ?2",
                             (id_to_unfollow, &undo_activity.actor))?;

    if res == 0 {
        response::not_found()
    } else {
        response::ok()
    }
}

#[cfg(test)]
mod tests {
    use http::StatusCode;
    use url::Url;
    use crate::activitypub::objects::activities::undo::UndoActivity;
    use crate::activitypub::objects::activities::Activity;
    use crate::activitypub::{AtContext, Context};
    use crate::local::actors::LocalActorStub;
    use crate::tests::data::EXT_ACTOR_1;
    use crate::tests::test_server::TestServer;
    use crate::tests::get_named_memory_db;
    use crate::database::Database;
    use crate::tests::get_memory_db;
    use crate::tests::requests::create_authed_inbox_post;
    use super::{FollowActivity, FollowActivityType};

    const ACTIVITY_ID_1: &str = "https://example.com/activity/1";
    const ACTIVITY_ID_2: &str = "https://example.com/activity/2";

    const ACTOR_ID: &str = "https://example.com/_mainsail/actors/1";

    const PROFILE_REAL: &str = "https://example.com/_mainsail/actors/1";
    const PROFILE_NOT_REAL: &str = "https://example.com/profiles/2";

    #[tokio::test]
    async fn send_follow() {
        let (db, name) = get_memory_db();
        let internal_actor = LocalActorStub::get(&db, ACTOR_ID).unwrap();

        let mut test_server = TestServer::new(&name).await;
        let actor_to_follow = test_server.new_actor();
        test_server.set_inbox_response(&actor_to_follow, 200).await;

        let inbox_url = actor_to_follow.inbox.parse().unwrap();
        let req_db = get_named_memory_db(&name);
        super::send(req_db, internal_actor, &actor_to_follow.id, inbox_url).await.unwrap();

        assert_eq!(num_following_pending(&db), 1);
        let requests = test_server.received_requests().await.unwrap();
        let sent_activity: FollowActivity = requests[0].body_json().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(sent_activity.actor, PROFILE_REAL);
        assert_eq!(sent_activity.object, actor_to_follow.id);
    }

    #[tokio::test]
    async fn send_follow_twice() {
        let (db, name) = get_memory_db();
        let internal_actor = LocalActorStub::get(&db, ACTOR_ID).unwrap();

        let mut test_server = TestServer::new(&name).await;
        let actor_to_follow = test_server.new_actor();
        test_server.set_inbox_response(&actor_to_follow, 200).await;

        let inbox_url: Url = actor_to_follow.inbox.parse().unwrap();
        let req_db_1 = get_named_memory_db(&name);
        let req_db_2 = get_named_memory_db(&name);

        super::send(req_db_1, internal_actor.clone(), &actor_to_follow.id, inbox_url.clone()).await.unwrap();
        super::send(req_db_2, internal_actor, &actor_to_follow.id, inbox_url).await.unwrap();

        assert_eq!(num_following_pending(&db), 1);
        let requests = test_server.received_requests().await.unwrap();
        let sent_activity: FollowActivity = requests[0].body_json().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(sent_activity.actor, PROFILE_REAL);
        assert_eq!(sent_activity.object, actor_to_follow.id);
    }

    #[tokio::test]
    async fn receive_follow() {
        let (db, name) = get_memory_db();
        let internal_actor = LocalActorStub::get(&db, ACTOR_ID).unwrap();
        let mut test_server = TestServer::new(&name).await;
        let external_actor = test_server.new_actor();

        let object = mock_follow(ACTIVITY_ID_1, &external_actor.id, &internal_actor.id);
        let req = create_authed_inbox_post(object.json(), &name, &external_actor);

        let res = super::receive(req, object).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(num_followers(&db), 1);
    }

    #[tokio::test]
    async fn receive_follow_twice_same_activity() {
        let (db, name) = get_memory_db();
        let internal_actor = LocalActorStub::get(&db, ACTOR_ID).unwrap();

        let mut test_server = TestServer::new(&name).await;
        let external_actor = test_server.new_actor();

        let object_1 = mock_follow(ACTIVITY_ID_1, &external_actor.id, &internal_actor.id);
        let object_2 = mock_follow(ACTIVITY_ID_1, &external_actor.id, &internal_actor.id);

        let req_1 = create_authed_inbox_post(object_1.json(), &name, &external_actor);
        let req_2 = create_authed_inbox_post(object_2.json(), &name, &external_actor);

        let res_1 = super::receive(req_1, object_1).await.unwrap();
        let res_2 = super::receive(req_2, object_2).await.unwrap();

        assert_eq!(res_1.status(), StatusCode::OK);
        assert_eq!(res_2.status(), StatusCode::OK);
        assert_eq!(num_followers(&db), 1);
    }

    #[tokio::test]
    async fn receive_follow_twice_different_activity() {
        let (db, name) = get_memory_db();
        let internal_actor = LocalActorStub::get(&db, ACTOR_ID).unwrap();

        let mut test_server = TestServer::new(&name).await;
        let external_actor = test_server.new_actor();

        let object_1 = mock_follow(ACTIVITY_ID_1, &external_actor.id, &internal_actor.id);
        let object_2 = mock_follow(ACTIVITY_ID_1, &external_actor.id, &internal_actor.id);

        let req_1 = create_authed_inbox_post(object_1.json(), &name, &external_actor);
        let req_2 = create_authed_inbox_post(object_2.json(), &name, &external_actor);

        let res_1 = super::receive(req_1, object_1).await.unwrap();
        let res_2 = super::receive(req_2, object_2).await.unwrap();

        assert_eq!(res_1.status(), StatusCode::OK);
        assert_eq!(res_2.status(), StatusCode::OK);
        assert_eq!(num_followers(&db), 1);
    }

    // This is a weird one because what should actually happen is that the wrapper should
    // "know" all the profiles before they get here. This just tests that they get ignored if
    // they're not known for some reason. I'm not sure that it's even necessary
    #[tokio::test]
    async fn receive_follow_profile_doesnt_exist() {
        let (db, name) = get_memory_db();

        let mut test_server = TestServer::new(&name).await;
        let external_actor = test_server.new_actor();

        let object = mock_follow(ACTIVITY_ID_1, &external_actor.id, PROFILE_NOT_REAL);
        let req = create_authed_inbox_post(object.json(), &name, &external_actor);
        let res = super::receive(req, object).await.unwrap_err();

        assert_eq!(res.status(), 404);
        assert_eq!(num_followers(&db), 0);
    }

    #[tokio::test]
    async fn undo_send_follow() {
        let (db, name) = get_memory_db();
        let internal_actor = LocalActorStub::get(&db, ACTOR_ID).unwrap();

        let mut test_server = TestServer::new(&name).await;
        let actor_to_follow = test_server.new_actor();
        test_server.set_inbox_response(&actor_to_follow, 200).await;

        // Send the follow
        let inbox_url = actor_to_follow.inbox.parse().unwrap();
        let req_db = get_named_memory_db(&name);
        super::send(req_db, internal_actor.clone(), &actor_to_follow.id, inbox_url).await.unwrap();
        assert_eq!(num_following_pending(&db), 1);

        // Undo the follow and ensure that it's deleted
        let undo_db = get_named_memory_db(&name);
        super::undo_send(undo_db, internal_actor, &actor_to_follow.id).await.unwrap();

        assert_eq!(num_following_pending(&db), 0);
        let requests = test_server.received_requests().await.unwrap();
        let undo_activity: UndoActivity<FollowActivity> = requests[1].body_json().unwrap();
        assert_eq!(requests.len(), 2);
        assert_eq!(undo_activity.actor, PROFILE_REAL);
        assert_eq!(undo_activity.object.object, actor_to_follow.id);
    }

    #[tokio::test]
    async fn undo_receive_follow() {
        let (db, name) = get_memory_db();
        let internal_actor = LocalActorStub::get(&db, ACTOR_ID).unwrap();

        let mut test_server = TestServer::new(&name).await;
        let external_actor = test_server.new_actor();

        let follow_object = mock_follow(ACTIVITY_ID_1, &external_actor.id, &internal_actor.id);
        let req = create_authed_inbox_post(follow_object.json(), &name, &external_actor);

        super::receive(req, follow_object.clone()).await.unwrap();
        let undo_object = new_undo(ACTIVITY_ID_2, &external_actor.id, follow_object);

        let req = create_authed_inbox_post(undo_object.json(), &name, &external_actor);
        let res = super::undo_receive(req, undo_object).unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(num_followers(&db), 0);
    }

    #[tokio::test]
    async fn undo_receive_follow_not_found() {
        let (db, name) = get_memory_db();
        let internal_actor = LocalActorStub::get(&db, ACTOR_ID).unwrap();

        let mut test_server = TestServer::new(&name).await;
        let external_actor = test_server.new_actor();

        let follow_object = mock_follow(ACTIVITY_ID_1, &external_actor.id, &internal_actor.id);
        let undo_object = new_undo(ACTIVITY_ID_2, &external_actor.id, follow_object);

        let req = create_authed_inbox_post(undo_object.json(), &name, &external_actor);
        let res = super::undo_receive(req, undo_object).unwrap();

        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(num_followers(&db), 0);
    }

    #[tokio::test]
    async fn undo_receive_follow_undo_doesnt_match_follow_actor() {
        let (db, name) = get_memory_db();
        let internal_actor = LocalActorStub::get(&db, ACTOR_ID).unwrap();

        let mut test_server = TestServer::new(&name).await;
        let external_actor = test_server.new_actor();

        let follow_object = mock_follow(ACTIVITY_ID_1, &EXT_ACTOR_1.id, &internal_actor.id);
        let req = create_authed_inbox_post(follow_object.json(), &name, &external_actor);

        let res = super::receive(req, follow_object.clone()).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(num_followers(&db), 1);

        let undo_object = new_undo(ACTIVITY_ID_2, &external_actor.id, follow_object);
        let req = create_authed_inbox_post(undo_object.json(), &name, &external_actor);
        let res = super::undo_receive(req, undo_object).unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(num_followers(&db), 1);
    }

    fn num_followers(db: &Database) -> i64 {
        db.query_row(
            "SELECT count(actor_id) FROM followers WHERE actor_id = ?1",
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

    fn new_undo(activity_id: &str, actor_id: &str, object: FollowActivity) -> UndoActivity<FollowActivity> {
        UndoActivity::new(activity_id.to_owned(), actor_id.to_owned(), object)
    }

    fn mock_follow(id: &str, sender_id: &str, id_to_follow: &str) -> FollowActivity {
        FollowActivity {
            context: Some(AtContext::Context(Context::ActivityStreams)),
            activity_type: FollowActivityType::Follow,
            id: id.parse().unwrap(),
            actor: sender_id.to_owned(),
            object: id_to_follow.to_owned(),
        }
    }
}
