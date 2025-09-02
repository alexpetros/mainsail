//! ActivityPub [Delete Activity](https://www.w3.org/TR/activitystreams-vocabulary/#dfn-delete)
use serde::{Deserialize, Serialize};
use tracing::info;
use url::Url;
use crate::activitypub::{AtContext, Context};
use crate::server::ap_request::AuthedApRequest;
use crate::local::actors::LocalActorStub;
use crate::server::response::{self};
use crate::server::response::ServerResult;
use crate::database::Database;
use crate::gateway::federation;
use crate::local::InternalResult;
use crate::utils::new_activity_id;

use super::Activity;

#[derive(Debug, Serialize, Deserialize)]
enum DeleteActivityType { Delete }

#[derive(Debug, Serialize, Deserialize)]
enum TombstoneType { Tombstone }

#[derive(Debug, Serialize, Deserialize)]
pub struct Tombstone {
    pub id: String,
    #[serde(rename = "type")]
    _type: TombstoneType,
}

impl Tombstone {
    pub fn new(id: String) -> Self {
        Tombstone { id, _type: TombstoneType::Tombstone }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DeleteObject {
    Tombstone(Tombstone),
    ActivityId(String),
    Unknown(serde_json::Value),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DeleteActivity {
    #[serde(rename = "type")]
    activity_type: DeleteActivityType,
    #[serde(rename = "@context")]
    pub context: Option<AtContext>,
    pub id: Url,
    pub actor: String,
    pub object: DeleteObject,
}

impl DeleteActivity {
    pub fn new(note_id: String, sender: &LocalActorStub) -> Self {
        let tombstone = Tombstone::new(note_id);
        DeleteActivity {
            activity_type: DeleteActivityType::Delete,
            context: Some(AtContext::Context(Context::ActivityStreams)),
            id: new_activity_id(&sender.domain).unwrap(),
            actor: sender.id.clone(),
            object: DeleteObject::Tombstone(tombstone),
        }
    }
}

impl Activity for DeleteActivity {
    fn get_id(&self) -> &Url {
        &self.id
    }

    fn json(&self) -> String {
        serde_json::json!(self).to_string()
    }
}

pub fn send_all(db: &Database, sender: LocalActorStub, note_id: String) -> InternalResult<()> {
    info!("Deleting note {}", &note_id);
    let activity = DeleteActivity::new(note_id.clone(), &sender);
    db.execute("DELETE FROM notes WHERE id = ?1", (&note_id,))?;
    federation::federate_activity(db, sender, activity)?;
    Ok(())
}

pub fn receive(req: AuthedApRequest, delete_activity: DeleteActivity) -> ServerResult {
    let to_delete = match delete_activity.object {
        DeleteObject::Tombstone(t) => t,
        DeleteObject::ActivityId(id) => {
            info!("Ignored the request to delete activity {id}");
            return response::not_implemented();
        }
        DeleteObject::Unknown(_) => {
            info!("Ignored the following DELETE Activity: {:?}", delete_activity);
            return response::not_implemented();
        }
    };

    let changes = req.db.execute("DELETE FROM cache WHERE id = ?1", (to_delete.id,))?;

    if changes == 0 {
        response::not_found()
    } else {
        response::ok()
    }
}

#[cfg(test)]
mod tests {
    use http::StatusCode;

    use crate::activitypub::Context;
    use crate::database::Database;
    use crate::tests::requests::create_authed_post;
    use crate::tests::data::EXT_ACTOR_1;
    use crate::tests::data::EXT_NOTE_1;
    use crate::tests::get_memory_db;

    use super::*;

    const ACTIVITY_ID_1: &str = "https://example.com/activity/1";
    const ACTIVITY_ID_2: &str = "https://example.com/activity/2";
    const OBJECT_NOT_REAL: &str = "https://example.com/users/happy_hippo/bad_url";
    const ACTOR: &str = "https://example.com/users/happy_hippo";

    #[test]
    fn delete_post_found() {
        let (db, name) = get_memory_db();
        let delete = mock_delete(ACTIVITY_ID_1, ACTOR, &EXT_NOTE_1.id);
        let req = create_authed_post("/inbox", &delete.json(), &name, &EXT_ACTOR_1);

        let res = receive(req, delete).unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(num_external_notes(db), 0);
    }

    #[test]
    fn delete_post_twice_same_activity() {
        let (db, name) = get_memory_db();
        let delete_1 = mock_delete(ACTIVITY_ID_1, ACTOR, &EXT_NOTE_1.id);
        let delete_2 = mock_delete(ACTIVITY_ID_1, ACTOR, &EXT_NOTE_1.id);

        let req_1 = create_authed_post("/inbox", &delete_1.json(), &name, &EXT_ACTOR_1);
        let req_2 = create_authed_post("/inbox", &delete_2.json(), &name, &EXT_ACTOR_1);

        let res_1 = receive(req_1, delete_1).unwrap();
        let res_2 = receive(req_2, delete_2).unwrap();

        assert_eq!(res_1.status(), StatusCode::OK);
        assert_eq!(res_2.status(), 404);
        assert_eq!(num_external_notes(db), 0);
    }

    #[test]
    fn delete_post_twice_different_activity() {
        let (db, name) = get_memory_db();
        let delete_1 = mock_delete(ACTIVITY_ID_1, ACTOR, &EXT_NOTE_1.id);
        let delete_2 = mock_delete(ACTIVITY_ID_2, ACTOR, &EXT_NOTE_1.id);

        let req_1 = create_authed_post("/inbox", &delete_1.json(), &name, &EXT_ACTOR_1);
        let req_2 = create_authed_post("/inbox", &delete_2.json(), &name, &EXT_ACTOR_1);

        let res_1 = receive(req_1, delete_1).unwrap();
        let res_2 = receive(req_2, delete_2).unwrap();

        assert_eq!(res_1.status(), StatusCode::OK);
        assert_eq!(res_2.status(), 404);
        assert_eq!(num_external_notes(db), 0);
    }

    #[test]
    fn delete_post_not_found() {
        let (db, name) = get_memory_db();
        let delete = mock_delete(ACTIVITY_ID_1, ACTOR, OBJECT_NOT_REAL);
        let req = create_authed_post("/inbox", &delete.json(), &name, &EXT_ACTOR_1);
        let res = receive(req, delete).unwrap();

        assert_eq!(res.status(), 404);
        assert_eq!(num_external_notes(db), 1);
    }

    fn num_external_notes(db: Database) -> i64 {
        db.query_row(
            "SELECT count(id) FROM cache WHERE type = 'Note'",
            (),
            |r| { r.get(0) },
        ).unwrap()
    }

    pub fn mock_delete(id: &str, actor: &str, object_id: &str) -> DeleteActivity {
        let tombstone = Tombstone { id: object_id.to_owned(), _type: TombstoneType::Tombstone };
        DeleteActivity {
            activity_type: DeleteActivityType::Delete,
            context: Some(AtContext::Context(Context::ActivityStreams)),
            id: id.parse().unwrap(),
            actor: actor.to_owned(),
            object: DeleteObject::Tombstone(tombstone),
        }
    }
}
