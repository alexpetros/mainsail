use url::Url;
use tracing::{info, warn};
use openssl::pkey::{PKey, Public};
use rusqlite::Error::QueryReturnedNoRows;
use rusqlite::{named_params, OptionalExtension};
use crate::activitypub::objects::actor::Actor;
use crate::activitypub::objects::note::Note;
use crate::database::Database;
use crate::error::MainsailError;
use crate::local::actors::LocalActorStub;
use crate::local::InternalResult;
use crate::{gateway, query_row};
use crate::utils::deserialize_json;

pub async fn get_note(db: Database, note_id: &str) -> InternalResult<Note> {
    // TODO fetch it if we don't have it
    let cache_object: String = db.query_row(
        "SELECT object FROM cache WHERE id = ?1",
        [note_id],
        |row| { row.get("object") },
    )?;

    let note: Note = deserialize_json(&cache_object)?;
    Ok(note)
}

pub async fn get_note_by_internal_id(db: Database, note_id: &str) -> InternalResult<Note> {
    // TODO fetch it if we don't have it
    let cache_object: String = db.query_row(
        "SELECT object FROM cache WHERE rowid = ?1",
        [note_id],
        |row| { row.get("object") },
    )?;

    let note: Note = deserialize_json(&cache_object)?;
    Ok(note)
}

pub async fn get_external_actor_for_note(db: Database, sender: &LocalActorStub, note_id: &str) -> InternalResult<Actor> {
    // TODO this needs to work even if we don't know the note
    let actor_id: String = db.query_row(
        "SELECT actor FROM cache WHERE id = ?",
        (note_id,),
        |row| { row.get(0) },
    )?;

    get_external_actor(db, sender, &actor_id).await
}

pub async fn get_external_actor(mut db: Database, sender: &LocalActorStub, actor_id: &str) -> InternalResult<Actor> {
    let actor = get_external_actor_from_cache(&db, actor_id)?;
    if let Some(actor) = actor {
        info!("Actor {} found locally, returning cached version", actor.id);
        return Ok(actor);
    }

    info!("Actor at {} not found locally, searching for them", actor_id);
    let url = actor_id.parse::<Url>().map_err(|_| MainsailError::InvalidActorUri(actor_id.into()))?;
    // warn!("Invalid URI provided for self by {}@{}: {:?}", preferred_username, host, &actor_link);

    let actor: Actor = gateway::get_from_ap(&url, sender).await?;
    save_external_actor(&mut db, &actor)?;

    Ok(actor)
}

pub fn get_external_actor_from_cache(db: &Database, actor_id: &str) -> InternalResult<Option<Actor>> {
    let res = query_row!(db, R { object: String }, "FROM cache WHERE id = ?1", (actor_id,));

    if let Err(QueryReturnedNoRows) = res {
        return Ok(None);
    }

    let actor_text = res?.object;
    let actor: Actor = deserialize_json(&actor_text)?;

    Ok(Some(actor))
}

pub async fn get_actor_public_key(url: &Url, server: &LocalActorStub) -> InternalResult<PKey<Public>> {
    let res: Actor = gateway::get_from_ap(url, server).await?;
    let key_string = res.public_key.public_key_pem;
    let pkey = PKey::public_key_from_pem(key_string.as_bytes())?;
    Ok(pkey)
}

pub async fn get_external_actor_no_cache(sender: &LocalActorStub, url: &Url) -> InternalResult<Actor> {
    gateway::get_from_ap(url, sender).await
}

pub fn get_actor_for_external_note(db: &Database, note_id: &str) -> InternalResult<Option<String>> {
    let actor_id: Option<String> = db.query_row(
        "SELECT actor FROM cache WHERE id = ?1",
        [note_id],
        |row| { row.get(0) },
    ).optional()?;

    Ok(actor_id)
}

pub async fn save_external_note(db: Database, note: &Note, actor_id: &str, sender: &LocalActorStub) -> InternalResult<()> {
    // This first call ensures the actor is in the DB
    get_external_actor(db.clone_conn()?, sender, actor_id).await?;
    save_external_note_no_validation(&db, note, actor_id)
}

pub fn save_external_note_no_validation(db: &Database, note: &Note, actor_id: &str) -> InternalResult<()> {
    let object = note.json();
    let res = db.execute(
        "INSERT INTO cache (id,  actor, type, object) VALUES (?1, ?2, 'Note', ?3)",
        (&note.id, &actor_id, &object),
    );

    if res.is_err() {
        warn!("Failed to add following note to cache: {}", object);
    }

    Ok(())
}

pub fn save_external_actor(db: &mut Database, actor: &Actor) -> InternalResult<()> {
    // This needs to be done with the actual response
    let object = actor.json();
    db.execute(
        "INSERT INTO cache (id, actor, type, object)
        VALUES (:id, :id, :type, :object)
        ON CONFLICT (id)
        DO UPDATE SET actor = :id, type = :type, object = :object ",
        named_params! { ":id": &actor.id, ":type": &actor.actor_type, ":object": object },
    )?;
    Ok(())
}
