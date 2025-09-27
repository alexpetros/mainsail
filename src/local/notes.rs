use rusqlite::{OptionalExtension, Params};
use tracing::warn;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::activitypub::objects::tags::{Mention, Tag};
use crate::activitypub::objects::activities::create::CreateActivity;
use crate::activitypub::objects::note::Note;
use crate::error::MainsailError;
use crate::local::InternalResult;
use crate::database::Database;
use rusqlite::Error::QueryReturnedNoRows;
use MainsailError::InternalServerError;
use crate::gateway::federation::federate_note;
use crate::utils::{deserialize_json, now_formatted};
use super::actors::LocalActorStub;

#[derive(Debug, Serialize, Deserialize)]
pub struct InternalNote {
    pub note: Note,
    pub uuid: String,
}

impl InternalNote {
    pub fn into_note(self: InternalNote) -> Note {
        self.note
    }

    pub fn into_create(self: InternalNote) -> CreateActivity {
        CreateActivity::new_from_note(self.note)
    }
}

/// Convert an ActivityPub URL to a local note_id.
///
/// URLs are the standard ID format of AP objects, but we don't store notes with URLs in the
/// database, we store them with IDs. So we need to be able to translate between the two.
///
/// # Examples
///
/// ```
///  use mainsail::local::notes::url_to_id;
///  let url = "https://example.com/notes/3";
///  let id = url_to_id("example.com", url);
///  assert_eq!(id, Some(3));
///  ```
///
///  If the note doesn't parse from the given domain, it will return None
///
/// ```
///  use mainsail::local::notes::url_to_id;
///  let url = "https://example.com/posts/1102d-123123";
///  let id = url_to_id("example.com", url);
///  assert_eq!(id, None);
/// ```
pub fn url_to_id(domain: &str, url: &str) -> Option<i64> {
    let first_part = format!("https://{domain}/notes/");
    let parts = url.split_once(&first_part)?;
    let note_id = parts.1;

    let note_id = note_id.parse::<i64>();
    match note_id {
        Ok(id) => Some(id),
        Err(e) => {
            warn!("Error while parsing url {}: {}", url, e);
            None
        }
    }
}

/// Convert a Note ID to its external URL representation.
///
/// # Examples
///
/// ```
///  use mainsail::local::notes::note_id_to_url;
///  let url = note_id_to_url("example.com", "3");
///  assert_eq!(url, "https://example.com/notes/3");
/// ```
pub fn note_id_to_url(domain: &str, note_id: &str) -> String {
    format!("https://{domain}/notes/{note_id}")
}

pub fn note_exists(db: &Database, id: &str) -> InternalResult<bool> {
    let res = db.query_row("SELECT 1 FROM notes WHERE id = ?1", (id,), |_| {
        Ok(())
    });

    match res {
        Err(QueryReturnedNoRows) => Ok(false),
        Err(e) => Err(MainsailError::SQLError(e)),
        Ok(_) => Ok(true)
    }
}

fn create(db: &Database, sender: &LocalActorStub, content: &str, in_reply_to: Option<String>) -> InternalResult<InternalNote> {
    let uuid = Uuid::new_v4().to_string();
    create_with_uuid(db, uuid, sender, content, in_reply_to)
}

pub fn insert_single_note(db: &Database, sender: &LocalActorStub, note: Note) -> InternalResult<Note> {
    let mut query = db.prepare_cached(
        "INSERT INTO notes (id, uuid, actor_id, object) VALUES (?1, ?2, ?3, ?4)"
    )?;

    let mut note = note.clone();
    // TODO this is not good -- fix this interface ASAP
    let id = note_id_to_url(&sender.domain, &note.id);
    let uuid = note.id;
    note.id = id;
    query.execute((&note.id, &uuid, &note.attributed_to, note.json()))?;

    Ok(note)
}

pub fn insert_bulk_notes(db: &Database, sender: &LocalActorStub, notes: Vec<Note>) -> InternalResult<Vec<Note>> {
    let mut query = db.prepare(
        "INSERT INTO notes (id, uuid, actor_id, object) VALUES (?1, ?2, ?3, ?4)"
    )?;

    for note in &notes {
        let mut note = note.clone();
        // TODO this is not good -- fix this interface ASAP
        let id = note_id_to_url(&sender.domain, &note.id);
        let uuid = note.id;
        note.id = id;
        query.execute((&note.id, &uuid, &note.attributed_to, note.json()))?;
    }

    Ok(notes)
}

pub fn insert_bulk_notes_and_federate(db: &Database, sender: &LocalActorStub, notes: Vec<Note>) -> InternalResult<()> {
    for note in notes {
        let uuid = note.id.clone();
        let note = insert_single_note(db, sender, note)?;
        let internal_note = InternalNote { note, uuid };
        federate_note(db, sender.clone(), internal_note)?;
    }
    Ok(())
}

pub fn create_with_uuid(db: &Database, uuid: String, sender: &LocalActorStub, content: &str, in_reply_to: Option<String>) -> InternalResult<InternalNote> {
    let id = note_id_to_url(&sender.domain, &uuid);
    let published = now_formatted();
    let note = Note::from_internal(id.clone(), sender.id.clone(), content.to_string(), published, in_reply_to);
    let object = note.json();

    db.execute(
        "INSERT INTO notes (id, uuid, actor_id, object) VALUES (?1, ?2, ?3, ?4)",
        (&id, &uuid, &sender.id, object),
    )?;

    get_note(db, &uuid)?.ok_or(InternalServerError(format!("Error creating note {uuid}")))
}

pub fn create_and_federate(db: &Database, actor_id: &str, content: &str, in_reply_to: Option<String>) -> InternalResult<String> {
    let internal_actor = LocalActorStub::get(db, actor_id).ok_or_else(|| {
        MainsailError::BadRequest(format!("Actor {actor_id} not found"))
    })?;
    let note = create(db, &internal_actor, content, in_reply_to)?;
    let note_uuid = note.uuid.clone();

    federate_note(db, internal_actor, note)?;
    Ok(note_uuid)
}

pub fn get_actor_id_for_note(db: &Database, note_id: &str) -> InternalResult<Option<String>> {
    let note: Option<String> = db.query_row(
        "SELECT actor_id FROM notes WHERE id = ?1", [note_id], |row| { row.get(0) },
    ).optional()?;
    Ok(note)
}

pub fn get_note(db: &Database, note_uuid: &str) -> InternalResult<Option<InternalNote>> {
    let object = db.query_row(
        "SELECT object FROM notes WHERE notes.uuid = ?1",
        [note_uuid],
        |row| {
            let object: String = row.get("object")?;
            Ok(object)
        }).optional()?;

    let Some(object) = object else {
        return Ok(None)
    };

    let note: Note = deserialize_json(&object)?;
    let mut internal_note = InternalNote { note, uuid: note_uuid.to_string() };

    // Populate tag based on in_reply_to
    if let Some(in_reply_to) = internal_note.note.in_reply_to.as_ref() {
        let replied_to_actor: String = db.query_row_and_then(
            "SELECT actor FROM cache WHERE id = ?1",
            [in_reply_to],
            |row| row.get("actor"),
        )?;

        internal_note.note.cc.push(replied_to_actor.clone());
        let mention = Mention::new(replied_to_actor);
        internal_note.note.tag.push(Tag::Mention(mention));
    };

    Ok(Some(internal_note))
}

pub fn get_notes_by_actor_id(db: &Database, actor_id: &str) -> InternalResult<Vec<InternalNote>> {
    let stmt = "SELECT uuid, object FROM notes WHERE notes.actor_id = ?1 ORDER BY object -> 'published' DESC";
    let params = [actor_id];
    query_notes(db, stmt, params)
}

fn query_notes<P: Params>(db: &Database, stmt: &str, params: P) -> Result<Vec<InternalNote>, MainsailError> {
    struct RawNote {

        uuid: String,
        object: String,
    }

    let mut stmt = db.prepare(stmt)?;
    let rows = stmt.query_map(params, |row| {
        let uuid: String = row.get("uuid")?;
        let object: String = row.get("object")?;
        let note = RawNote { uuid, object };
        Ok(note)
    })?;
    let rows: Vec<RawNote> = rows.collect::<Result<_, _>>()?;

    // TODO this could maybe be more efficient
    let mut notes = Vec::with_capacity(rows.len());
    for row in rows {
        let note: Note = deserialize_json(&row.object)?;
        let internal_note = InternalNote { uuid: row.uuid, note };
        notes.push(internal_note)
    }

    Ok(notes)
}

#[cfg(test)]
pub mod tests {
    use crate::tests::get_memory_db;
    const ACTOR_ID: &str = "https://example.com/_mainsail/actors/1";

    #[test]
    fn get_notes() {
        let (db, _) = get_memory_db();
        let notes = super::get_notes_by_actor_id(&db, ACTOR_ID).unwrap();
        assert_eq!(notes.len(), 3);

    }
}
