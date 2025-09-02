use serde::Deserialize;
use serde::Serialize;
use crate::activitypub::objects::actor::Actor;
use crate::activitypub::objects::note::Note;
use crate::database::Database;
use crate::error::MainsailError;
use crate::local::InternalResult;
use crate::utils::deserialize_json;

static FOLLOWING_TIMELINE: &str = "
        SELECT
            cache.object as note_object,
            actors.object as actor_object,
            json_object('internal_id', cast (cache.rowid as text)) as meta_object
        FROM cache
        LEFT JOIN following on following.followed_actor_id = cache.actor
        LEFT JOIN cache as actors on actors.id = cache.actor
        WHERE
            cache.type = 'Note' AND
            following.actor_id = ?1 AND
            following.activity_id IS NOT NULL
        ORDER BY json_extract(cache.object, '$.published') DESC
        LIMIT 50
    ";

static MUTUALS_TIMELINE: &str = "
        SELECT
            cache.object as note_object,
            actors.object as actor_object,
            json_object('internal_id', cast (cache.rowid as text)) as meta_object
        FROM cache
        LEFT JOIN following on following.followed_actor_id = cache.actor
        LEFT JOIN followers on followers.sender_id = cache.actor
        LEFT JOIN cache as actors on actors.id = cache.actor
        WHERE
            cache.type = 'Note' AND
            following.actor_id = ?1 AND
            following.activity_id IS NOT NULL AND
            followers.activity_id IS NOT NULL
        ORDER BY json_extract(cache.object, '$.published') DESC
        LIMIT 50
    ";

#[derive(Debug, Serialize, Deserialize)]
pub struct Meta {
    pub internal_id: String,
}

#[derive(Debug, Serialize)]
pub struct Post {
    note: Note,
    actor: Actor,
    meta: Meta,
}

impl Post {
    pub fn new(note: Note, actor: Actor, meta: Meta) -> Self {
        Post { note, actor, meta }
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum TimelineEntry {
    Post(Box<Post>),
    Thread(Vec<Post>),
}

struct RawPost
{
    note_object: String,
    actor_object: String,
    meta_object: String,
}

fn get_timeline(db: &Database, query: &str, actor_id: &str) -> InternalResult<Vec<TimelineEntry>> {
    // Get the most recent 50 notes from the users the profile follows
    // These will come straight from the cache for now
    let mut statement = db.prepare(query)?;

    let notes: Vec<RawPost> = statement.query_map((actor_id,), |row| {
        let note_object = row.get("note_object")?;
        let actor_object = row.get("actor_object")?;
        let meta_object = row.get("meta_object")?;
        Ok(RawPost { note_object, actor_object, meta_object })
    })?.collect::<Result<_, _>>()?;

    tracing::debug!("Retrieving {} notes for profile {actor_id}", notes.len());

    let items: Vec<Post> = notes.iter()
        .map(|raw_note| {
            let note: Note = deserialize_json(&raw_note.note_object)?;
            let actor: Actor = deserialize_json(&raw_note.actor_object)?;
            let meta: Meta = deserialize_json(&raw_note.meta_object)?;
            let item = Post { note, actor, meta };
            Ok(item)
        })
        .collect::<Result<_, MainsailError>>()?;

    let entries = items.into_iter().map(|note| { TimelineEntry::Post(note.into()) }).collect();
    Ok(entries)
}

pub fn following_timeline(db: &Database, actor_id: &str) -> InternalResult<Vec<TimelineEntry>> {
    get_timeline(db, FOLLOWING_TIMELINE, actor_id)
}

pub fn mutuals_timeline(db: &Database, actor_id: &str) -> InternalResult<Vec<TimelineEntry>> {
    get_timeline(db, MUTUALS_TIMELINE, actor_id)
}
