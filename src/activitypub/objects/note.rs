//! ActivityPub Note Object
//!
//! https://www.w3.org/TR/activitystreams-vocabulary/#dfn-note
use serde::{Deserialize, Serialize};

use crate::activitypub::objects::activities::create::CreateActivity;
use crate::PUBLIC_STREAM;

use super::tags;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum NoteType {
    Note
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Note {
    pub id: String,
    #[serde(rename = "type")]
    pub _type: NoteType,
    pub url: String,
    pub summary: Option<String>,
    pub published: Option<String>,
    #[serde(rename = "inReplyTo")]
    pub in_reply_to: Option<String>,
    #[serde(rename = "attributedTo")]
    pub attributed_to: String,
    #[serde(default)]
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    #[serde(default)] // default false
    pub sensitive: bool,
    pub content: String,
    #[serde(default)]
    pub attachment: Vec<Attachment>,
    #[serde(default)]
    pub tag: Vec<tags::Tag>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum AttachmentType {
    Document
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Attachment {
    #[serde(rename = "type")]
    _type: AttachmentType,
    #[serde(rename = "mediaType")]
    media_type: String,
    url: String,
    name: Option<String>,
    blurhash: Option<String>,
    width: i64,
    height: i64,
}

impl Note {

    pub fn from_internal(id: String, actor_id: String, content: String, published: String, in_reply_to: Option<String>) -> Self {
        let cc = vec!(format!("{}/followers", &actor_id));
        Note {
            id: id.clone(),
            _type: NoteType::Note,
            url: id,
            summary: None,
            published: Some(published),
            attributed_to: actor_id,
            in_reply_to,
            attachment: vec![],
            to: vec![PUBLIC_STREAM.to_owned()],
            cc,
            sensitive: false,
            content,
            tag: vec!(),
        }
    }

    pub fn into_create(self: Note) -> CreateActivity {
        CreateActivity::new_from_note(self)
    }

    pub fn json(&self) -> String {
        serde_json::json!(self).to_string()
    }
}

#[cfg(test)]
mod tests {
    // const ACTOR_ID: &str = "https://social.example.com/users/alex";

    // TODO replace with new data
    // #[test]
    // fn from_json_note_with_attachment() {
    //     let json = include_str!("../../tests/data/note-with-attachment.json");
    //     let note: Note = serde_json::from_str(json).unwrap();
    //     assert_eq!(&note.id, "https://social.example.com/users/alex/statuses/11111111111");

    //     let (mut db, _) = get_memory_db_no_test_data();
    //     let actor = new_test_actor(ACTOR_ID, "Alex", "alex");
    //     external::save_external_actor(&mut db, &actor).unwrap();
    //     save_external_note_no_validation(&db, &note, ACTOR_ID).unwrap();

    //     assert_eq!(num_external_notes(&db), 1);
    // }

    // fn num_external_notes(db: &Database) -> i64 {
    //     db.query_row("SELECT count(*) FROM cache WHERE type = 'Note'", (), |r| { r.get(0) }).unwrap()
    // }
}
