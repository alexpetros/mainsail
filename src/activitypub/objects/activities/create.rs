//! ActivityPub [Create Activity](https://www.w3.org/TR/activitystreams-vocabulary/#dfn-create)
use serde::{Deserialize, Serialize};
use tracing::info;
use url::Url;
use crate::activitypub::{AtContext, Context};
use crate::activitypub::objects::Object;
use crate::activitypub::objects::activities::undo::UndoActivity;
use crate::activitypub::objects::note::Note;
use crate::server::ap_request::AuthedApRequest;
use crate::local::notes::note_exists;
use crate::local::notifications::insert_reply_notification;
use crate::{external, get_server_internal_actor, PUBLIC_STREAM};
use crate::external::get_actor_for_external_note;
use crate::server::response;
use crate::server::response::ServerResult;

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateActivity {
    #[serde(rename = "type")]
    _type: CreateActivityType,
    #[serde(rename = "@context")]
    pub context: Option<AtContext>,
    pub id: Url,
    pub actor: String,
    pub published: Option<String>,
    #[serde(default)]
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    pub object: Object,
}

#[derive(Debug, Serialize, Deserialize)]
enum CreateActivityType { Create }

impl CreateActivity {
    pub fn json(&self) -> String {
        serde_json::json!(self).to_string()
    }

    pub fn new_from_note(note: Note) -> Self {
        CreateActivity {
            context: Some(AtContext::Context(Context::ActivityStreams)),
            _type: CreateActivityType::Create,
            id: note.url.parse().unwrap(),
            actor: note.attributed_to.to_owned(),
            published: note.published.to_owned(),
            to: vec![PUBLIC_STREAM.to_owned()],
            cc: note.cc.clone(),
            object: Object::Note(note),
        }
    }
}

#[tracing::instrument(skip_all)]
pub async fn receive(req: AuthedApRequest, create_activity: CreateActivity) -> ServerResult {
    let note = match create_activity.object {
        Object::Note(n) => n,
        _ => {
            info!("Received and ignored the following CREATE request: {}", req.text()?);
            return response::not_implemented();
        }
    };

    let server = get_server_internal_actor(&req.db)?;
    external::save_external_note(req.db.clone_conn()?, &note, &create_activity.actor, &server).await?;
    // TODO rewrite with if let chains for rust 2024
    if let Some(in_reply_to) = &note.in_reply_to {
        let exists = note_exists(&req.db, in_reply_to)?;
        if exists {
            insert_reply_notification(&req.db, &note, &create_activity.id)?;
        }
    }

    response::ok()
}

pub fn undo_receive(req: AuthedApRequest, activity: UndoActivity<CreateActivity>) -> ServerResult {
    let note_id = &activity.object.id;
    let note_actor_id = get_actor_for_external_note(&req.db, note_id.as_str())?;
    let Some(note_actor_id) = note_actor_id else {
        return response::not_found()
    };

    if note_actor_id != req.actor.id {
        return response::not_found();
    }

    req.db.execute("DELETE FROM cache WHERE id = ?1", (&activity.object.id.as_str(),))?;
    response::ok()
}

#[cfg(test)]
mod tests {
    use http::StatusCode;
    use crate::activitypub::objects::activities::Activity;
    use crate::activitypub::objects::activities::create::CreateActivity;
    use crate::activitypub::objects::activities::undo::UndoActivity;
    use crate::tests::data::{EXT_ACTOR_1, EXT_ACTOR_2, new_test_note};
    use crate::database::Database;
    use crate::tests::get_memory_db;
    use crate::tests::requests::create_authed_post;

    const NOTE_URL: &str = "https://example.com/users/happy_hippo/statuses/10";
    const CONTENT: &str = "This is a test note";

    const ACTIVITY_ID: &str = "https://example.com/activity/1";

    // TODO add these back with generated test data
    // #[test]
    // fn from_json_create_note() {
    //     let json = include_str!("../../../tests/data/create-note.json");
    //     let note: CreateActivity = serde_json::from_str(json).unwrap();
    //     assert_eq!(note.id.as_str(), "https://social.example.com/users/alex/statuses/11111111111");
    // }

    // #[test]
    // fn from_json_create_note_reply() {
    //     let json = include_str!("../../../tests/data/create-note-reply.json");
    //     let note: CreateActivity = serde_json::from_str(json).unwrap();
    //     assert_eq!(note.id.as_str(), "https://social.example.com/users/other/statuses/11111111111");
    // }

    #[tokio::test]
    async fn create() {
        let (db, name) = get_memory_db();
        let object = new_test_note(NOTE_URL, &EXT_ACTOR_1.id, CONTENT).into_create();
        let req = create_authed_post("/inbox", &object.json(), &name, &EXT_ACTOR_1);

        let res = super::receive(req, object).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(num_external_notes(&db), 2);
    }

    #[tokio::test]
    async fn create_note_twice() {
        let (db, name) = get_memory_db();
        let object_1 = new_test_note(NOTE_URL, &EXT_ACTOR_1.id, CONTENT).into_create();
        let object_2 = new_test_note(NOTE_URL, &EXT_ACTOR_1.id, CONTENT).into_create();

        let req_1 = create_authed_post("/inbox", &object_1.json(), &name, &EXT_ACTOR_1);
        let req_2 = create_authed_post("/inbox", &object_2.json(), &name, &EXT_ACTOR_1);

        let res_1 = super::receive(req_1, object_1).await.unwrap();
        let res_2 = super::receive(req_2, object_2).await.unwrap();

        assert_eq!(res_1.status(), StatusCode::OK);
        assert_eq!(res_2.status(), StatusCode::OK);
        assert_eq!(num_external_notes(&db), 2);
    }

    #[tokio::test]
    async fn undo_create() {
        let (db, name) = get_memory_db();
        let create_object = new_test_note(NOTE_URL, &EXT_ACTOR_1.id, CONTENT).into_create();

        let req = create_authed_post("/inbox", &create_object.json(), &name, &EXT_ACTOR_1);
        let res = super::receive(req, create_object).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(num_external_notes(&db), 2);

        let create_object = new_test_note(NOTE_URL, &EXT_ACTOR_1.id, CONTENT).into_create();
        let undo_object = new_undo(ACTIVITY_ID, &EXT_ACTOR_1.id, create_object);
        let req = create_authed_post("/inbox", &undo_object.json(), &name, &EXT_ACTOR_1);
        let res = super::undo_receive(req, undo_object).unwrap();

        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(num_external_notes(&db), 1);
    }

    #[tokio::test]
    async fn undo_create_wrong_actor() {
        let (db, name) = get_memory_db();
        let create_object = new_test_note(NOTE_URL, &EXT_ACTOR_1.id, CONTENT).into_create();

        let req = create_authed_post("/inbox", &create_object.json(), &name, &EXT_ACTOR_1);
        let res = super::receive(req, create_object).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(num_external_notes(&db), 2);

        let create_object = new_test_note(NOTE_URL, &EXT_ACTOR_1.id, CONTENT).into_create();
        let undo_object = new_undo(ACTIVITY_ID, &EXT_ACTOR_2.id, create_object);
        let req = create_authed_post("/inbox", &undo_object.json(), &name, &EXT_ACTOR_2);
        let res = super::undo_receive(req, undo_object).unwrap();

        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(num_external_notes(&db), 2);
    }

    #[tokio::test]
    async fn undo_create_not_found() {
        let (db, name) = get_memory_db();

        let create_object = new_test_note(NOTE_URL, &EXT_ACTOR_1.id, CONTENT).into_create();
        let undo_object = new_undo(ACTIVITY_ID, &EXT_ACTOR_1.id, create_object);
        let req = create_authed_post("/inbox", &undo_object.json(), &name, &EXT_ACTOR_1);
        let req = super::undo_receive(req, undo_object).unwrap();

        assert_eq!(req.status(), StatusCode::NOT_FOUND);
        assert_eq!(num_external_notes(&db), 1);
    }

    fn new_undo(activity_id: &str, actor_id: &str, object: CreateActivity) -> UndoActivity<CreateActivity> {
        UndoActivity::new(activity_id.to_owned(), actor_id.to_owned(), object)
    }

    fn num_external_notes(db: &Database) -> i64 {
        db.query_row(
            "SELECT count(*) FROM cache WHERE type = 'Note'",
            (),
            |r| { r.get(0) },
        ).unwrap()
    }
}
