//! ActivityPub [Announce Activity](https://www.w3.org/TR/activitystreams-vocabulary/#dfn-announce)
use serde::{Deserialize, Serialize};
use tracing::warn;
use url::Url;
use crate::activitypub::{AtContext, UrlOrObject};
use crate::activitypub::objects::activities::undo::UndoActivity;
use crate::activitypub::ApUrl;
use crate::activitypub::objects::note::Note;
use crate::server::ap_request::AuthedApRequest;
use crate::local::notes::note_exists;
use crate::local::notifications::insert_announce_notification;
use crate::server::response;
use crate::server::response::ServerResult;

use super::Activity;

#[derive(Debug, Serialize, Deserialize)]
pub struct AnnounceActivity {
    #[serde(rename = "@context")]
    pub context: Option<AtContext>,
    pub id: Url,
    #[serde(rename = "type")]
    pub _type: AnnounceType,
    pub actor: ApUrl,
    pub object: UrlOrObject<Note>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum AnnounceType {
    Announce
}

impl Activity for AnnounceActivity {
    fn get_id(&self) -> &Url {
        &self.id
    }

    fn json(&self) -> String {
        serde_json::json!(self).to_string()
    }
}

pub fn receive(req: AuthedApRequest, activity: AnnounceActivity) -> ServerResult {
    let id = match &activity.object {
        UrlOrObject::Url(url) => url,
        UrlOrObject::Object(obj) => {
            warn!("Not currently announcing embedded objects. Received object: {:?}", obj);
            return response::not_implemented();
        }
    };

    if !note_exists(&req.db, id)? {
        return response::not_found();
    }

    let _ = req.db.execute(
        "INSERT OR REPLACE INTO note_announcements (activity_id, note_id, actor_id)
        VALUES (?1, ?2, ?3)",
        (&activity.id.as_str(), id, &activity.actor))?;

    insert_announce_notification(&req.db, id, &activity)?;

    response::ok()
}

pub fn undo_receieve(req: AuthedApRequest, undo_activity: UndoActivity<AnnounceActivity>) -> ServerResult {
    // Eventually you will need to verify that they are authorized to do the deletion
    req.db.execute("DELETE FROM note_announcements WHERE activity_id = ?1", (&undo_activity.object.id.as_str(),))?;
    response::ok()
}

#[cfg(test)]
mod tests {
    use http::StatusCode;

    use crate::activitypub::UrlOrObject;
    use crate::activitypub::BASIC_CONTEXT;
    use crate::activitypub::objects::activities::Activity;
    use crate::tests::requests::create_authed_inbox_post;
    use crate::tests::data::EXT_ACTOR_1;
    use crate::utils::new_activity_id;
    use crate::database::Database;
    use crate::tests::get_memory_db;

    use super::{AnnounceActivity, AnnounceType};

    const ACTIVITY_ID_1: &str = "https://example.com/activity/1";
    const ACTIVITY_ID_2: &str = "https://example.com/activity/2";
    const OBJECT_REAL: &str = "https://example.com/notes/1";
    const OBJECT_NOT_REAL: &str = "https://example.com/notes/10";

    #[test]
    fn announce_post() {
        let (db, name) = get_memory_db();
        let object = mock_announce(ACTIVITY_ID_1, &EXT_ACTOR_1.id, OBJECT_REAL);
        let req = create_authed_inbox_post(object.json(), &name, &EXT_ACTOR_1);

        let res = super::receive(req, object).unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(num_announcements(db), 1);
    }

    #[test]
    fn announce_post_twice_same_activity() {
        let (db, name) = get_memory_db();
        let object_1 = mock_announce(ACTIVITY_ID_1, &EXT_ACTOR_1.id, OBJECT_REAL);
        let object_2 = mock_announce(ACTIVITY_ID_1, &EXT_ACTOR_1.id, OBJECT_REAL);

        let req_1 = create_authed_inbox_post(object_1.json(), &name, &EXT_ACTOR_1);
        let req_2 = create_authed_inbox_post(object_2.json(), &name, &EXT_ACTOR_1);

        let res_1 = super::receive(req_1, object_1).unwrap();
        let res_2 = super::receive(req_2, object_2).unwrap();

        assert_eq!(res_1.status(), StatusCode::OK);
        assert_eq!(res_2.status(), StatusCode::OK);
        assert_eq!(num_announcements(db), 1);
    }

    #[test]
    fn announce_post_twice_different_activity() {
        let (db, name) = get_memory_db();
        let object_1 = mock_announce(ACTIVITY_ID_1, &EXT_ACTOR_1.id, OBJECT_REAL);
        let object_2 = mock_announce(ACTIVITY_ID_2, &EXT_ACTOR_1.id, OBJECT_REAL);

        let req_1 = create_authed_inbox_post(object_1.json(), &name, &EXT_ACTOR_1);
        let req_2 = create_authed_inbox_post(object_2.json(), &name, &EXT_ACTOR_1);

        let res_1 = super::receive(req_1, object_1).unwrap();
        let res_2 = super::receive(req_2, object_2).unwrap();

        assert_eq!(res_1.status(), StatusCode::OK);
        assert_eq!(res_2.status(), StatusCode::OK);
        assert_eq!(num_announcements(db), 1);
    }

    #[test]
    fn announce_post_not_found() {
        let (db, name) = get_memory_db();
        let object = mock_announce(ACTIVITY_ID_1, &EXT_ACTOR_1.id, OBJECT_NOT_REAL);
        let req = create_authed_inbox_post(object.json(), &name, &EXT_ACTOR_1);
        let res = super::receive(req, object).unwrap();

        assert_eq!(res.status(), 404);
        assert_eq!(num_announcements(db), 0);
    }

    fn mock_announce(domain: &str, actor: &str, announced_object: &str) -> AnnounceActivity {
        AnnounceActivity {
            context: BASIC_CONTEXT,
            id: new_activity_id(domain).unwrap(),
            actor: actor.to_owned(),
            _type: AnnounceType::Announce,
            object: UrlOrObject::Url(announced_object.to_owned()),
        }
    }

    fn num_announcements(db: Database) -> i64 {
        db.query_row(
            "SELECT count(activity_id) FROM note_announcements",
            (),
            |r| { r.get(0) },
        ).unwrap()
    }
}
