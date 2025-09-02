//! ActivityPub [Like Activity](https://www.w3.org/TR/activitystreams-vocabulary/#dfn-like)
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use url::Url;
use crate::activitypub::{AtContext, Context};
use crate::activitypub::objects::activities::undo::UndoActivity;
use crate::activitypub::ApUrl;
use crate::server::ap_request::AuthedApRequest;
use crate::local::actors::LocalActorStub;
use crate::local::notes::note_exists;
use crate::local::notifications::insert_like_notification;
use crate::server::response::{self, ServerResult};
use crate::database::Database;
use crate::gateway;
use crate::local::InternalResult;
use crate::utils::new_activity_id;

use super::Activity;

#[derive(Debug, Serialize, Deserialize)]
pub enum LikeType {
    Like
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LikeActivity {
    #[serde(rename = "@context")]
    pub context: Option<AtContext>,
    pub id: Url,
    #[serde(rename = "type")]
    pub _type: LikeType,
    pub actor: ApUrl,
    pub object: ApUrl,
}

impl LikeActivity {
    pub fn new_internal(domain: &str, actor_id: String, liked_object: &str) -> LikeActivity {
        LikeActivity {
            context: Some(AtContext::Context(Context::ActivityStreams)),
            id: new_activity_id(domain).unwrap(),
            actor: actor_id.to_string(),
            _type: LikeType::Like,
            object: liked_object.to_string(),
        }
    }

    pub fn new_external(id: &str, sender_id: &str, liked_object: &str) -> LikeActivity {
        LikeActivity {
            context: Some(AtContext::Context(Context::ActivityStreams)),
            id: id.parse().unwrap(),
            actor: sender_id.to_owned(),
            _type: LikeType::Like,
            object: liked_object.to_string(),
        }
    }
}

impl Activity for LikeActivity {
    fn get_id(&self) -> &Url {
        &self.id
    }

    fn json(&self) -> String {
        serde_json::json!(self).to_string()
    }
}

pub async fn send(db: Database, sender: LocalActorStub, note_id: &str, inbox_url: Url) -> InternalResult<()> {
    tracing::debug!("Sending like to for note {} to {}", note_id, inbox_url);
    let activity = LikeActivity::new_internal(&sender.domain, sender.id.clone(), note_id);
    db.execute(
        "INSERT INTO likes (activity_id, actor_id, note_id) VALUES (?1, ?2, ?3)",
        (activity.id.as_str(), &sender.id, note_id),
    )?;

    gateway::send_activity(db, inbox_url, sender, &activity).await
}

pub fn receive(req: AuthedApRequest, like_activity: LikeActivity) -> ServerResult {
    let note_id = &like_activity.object;
    if !note_exists(&req.db, note_id)? {
        return response::not_found();
    }

    let _ = req.db.execute(
        "INSERT OR REPLACE INTO note_likes (activity_id, note_id, actor_id)
        VALUES (?1, ?2, ?3)",
        (like_activity.id.as_str(), note_id, &like_activity.actor))?;
    insert_like_notification(&req.db, note_id, &like_activity)?;

    response::ok()
}

pub fn undo_receive(req: AuthedApRequest, undo_activity: UndoActivity<LikeActivity>) -> ServerResult {
    // Eventually you will need to verify that they are authorized to do the deletion
    let like_activity_id = &undo_activity.object.id;
    let like_actor_id: Option<String> = req.db.query_row(
        "SELECT actor_id FROM note_likes WHERE activity_id = ?1",
        [like_activity_id.as_str()],
        |row| { row.get(0) },
    )
        .optional()?;

    let Some(like_actor_id) = like_actor_id else {
        return response::not_found();
    };

    if like_actor_id != req.actor.id {
        return response::not_found();
    };

    req.db.execute("DELETE FROM note_likes WHERE activity_id = ?1", (&undo_activity.object.id.as_str(),))?;
    response::ok()
}

#[cfg(test)]
mod tests {
    use http::StatusCode;

    use crate::activitypub::objects::activities::Activity;
    use crate::external::save_external_note_no_validation;

    use crate::tests::test_server::TestServer;
    use crate::tests::get_named_memory_db;
    use crate::database::Database;

    use crate::tests::data::{EXT_ACTOR_1, EXT_ACTOR_2, EXT_NOTE_1};
    use crate::tests::get_memory_db;
    use crate::tests::requests::create_authed_inbox_post;
    use super::*;

    const ACTIVITY_ID_1: &str = "https://example.com/activity/1";
    const ACTIVITY_ID_2: &str = "https://example.com/activity/2";

    const OBJECT_REAL: &str = "https://example.com/notes/1";
    const OBJECT_NOT_REAL: &str = "https://example.com/notes/10";

    const ACTOR_ID: &str = "https://example.com/_mainsail/actors/1";

    #[tokio::test]
    async fn send_like() {
        let (db, name) = get_memory_db();
        let sender = LocalActorStub::get(&db, ACTOR_ID).unwrap();

        let mut test_server = TestServer::new(&name).await;
        let ext_actor = test_server.new_actor();
        save_external_note_no_validation(&db, &EXT_NOTE_1, &EXT_ACTOR_1.id).unwrap();
        test_server.set_inbox_response(&ext_actor, 200).await;

        let inbox_url = ext_actor.inbox.parse().unwrap();
        let req_db = get_named_memory_db(&name);
        super::send(req_db, sender, &EXT_NOTE_1.id, inbox_url).await.unwrap();

        assert_eq!(num_likes_sent(&db), 1);

        let requests = test_server.received_requests().await.unwrap();
        let sent_activity: LikeActivity = requests[0].body_json().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(sent_activity.actor, ACTOR_ID);
        assert_eq!(sent_activity.object, EXT_NOTE_1.id);
    }

    #[test]
    fn receive_like_once() {
        let (db, name) = get_memory_db();
        let like = LikeActivity::new_external(ACTIVITY_ID_1, &EXT_ACTOR_1.id, OBJECT_REAL);
        let req = create_authed_inbox_post(like.json(), &name, &EXT_ACTOR_1);

        let res = receive(req, like).unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(num_likes_received(db), 1);
    }

    #[test]
    fn receive_like_twice_same_activity() {
        let (db, name) = get_memory_db();
        let like_1 = LikeActivity::new_external(ACTIVITY_ID_1, &EXT_ACTOR_1.id, OBJECT_REAL);
        let like_2 = LikeActivity::new_external(ACTIVITY_ID_1, &EXT_ACTOR_1.id, OBJECT_REAL);

        let req_1 = create_authed_inbox_post(like_1.json(), &name, &EXT_ACTOR_1);
        let req_2 = create_authed_inbox_post(like_2.json(), &name, &EXT_ACTOR_1);

        let res_1 = receive(req_1, like_1).unwrap();
        let res_2 = receive(req_2, like_2).unwrap();

        assert_eq!(res_1.status(), StatusCode::OK);
        assert_eq!(res_2.status(), StatusCode::OK);
        assert_eq!(num_likes_received(db), 1);
    }

    #[test]
    fn receive_like_twice_different_activity() {
        let (db, name) = get_memory_db();
        let like_1 = LikeActivity::new_external(ACTIVITY_ID_1, &EXT_ACTOR_1.id, OBJECT_REAL);
        let like_2 = LikeActivity::new_external(ACTIVITY_ID_2, &EXT_ACTOR_1.id, OBJECT_REAL);

        let req_1 = create_authed_inbox_post(like_1.json(), &name, &EXT_ACTOR_1);
        let req_2 = create_authed_inbox_post(like_2.json(), &name, &EXT_ACTOR_1);

        let res_1 = receive(req_1, like_1).unwrap();
        let res_2 = receive(req_2, like_2).unwrap();

        assert_eq!(res_1.status(), StatusCode::OK);
        assert_eq!(res_2.status(), StatusCode::OK);
        assert_eq!(num_likes_received(db), 1);
    }

    #[test]
    fn receive_like_not_found() {
        let (db, name) = get_memory_db();
        let like = LikeActivity::new_external(ACTIVITY_ID_1, &EXT_ACTOR_1.id, OBJECT_NOT_REAL);
        let req = create_authed_inbox_post(like.json(), &name, &EXT_ACTOR_1);
        let res = receive(req, like).unwrap();

        assert_eq!(res.status(), 404);
        assert_eq!(num_likes_received(db), 0);
    }

    #[test]
    fn undo_receive_like() {
        let (db, name) = get_memory_db();

        let like = LikeActivity::new_external(ACTIVITY_ID_1, &EXT_ACTOR_1.id, OBJECT_REAL);
        let req = create_authed_inbox_post(like.json(), &name, &EXT_ACTOR_1);
        receive(req, like).unwrap();

        let like = LikeActivity::new_external(ACTIVITY_ID_1, &EXT_ACTOR_1.id, OBJECT_REAL);
        let undo = new_undo(ACTIVITY_ID_2, &EXT_ACTOR_1.id, like);
        let req = create_authed_inbox_post(undo.json(), &name, &EXT_ACTOR_1);
        let res = undo_receive(req, undo).unwrap();

        assert_eq!(res.status(), 200);
        assert_eq!(num_likes_received(db), 0);
    }

    #[test]
    fn undo_receive_like_wrong_actor() {
        let (db, name) = get_memory_db();

        let like = LikeActivity::new_external(ACTIVITY_ID_1, &EXT_ACTOR_1.id, OBJECT_REAL);
        let req = create_authed_inbox_post(like.json(), &name, &EXT_ACTOR_1);
        receive(req, like).unwrap();

        let like = LikeActivity::new_external(ACTIVITY_ID_1, &EXT_ACTOR_1.id, OBJECT_REAL);
        let undo = new_undo(ACTIVITY_ID_2, &EXT_ACTOR_2.id, like);
        let req = create_authed_inbox_post(undo.json(), &name, &EXT_ACTOR_2);
        let res = undo_receive(req, undo).unwrap();

        assert_eq!(res.status(), 404);
        assert_eq!(num_likes_received(db), 1);
    }

    fn num_likes_received(db: Database) -> i64 {
        db.query_row(
            "SELECT count(activity_id) FROM note_likes",
            (),
            |r| { r.get(0) },
        ).unwrap()
    }

    fn num_likes_sent(db: &Database) -> i64 {
        db.query_row(
            "SELECT count(activity_id) FROM likes",
            (),
            |r| { r.get(0) },
        ).unwrap()
    }

    fn new_undo(activity_id: &str, actor_id: &str, object: LikeActivity) -> UndoActivity<LikeActivity> {
        UndoActivity::new(activity_id.to_owned(), actor_id.to_owned(), object)
    }
}
