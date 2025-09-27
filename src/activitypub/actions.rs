use serde::de::DeserializeOwned;
use url::Url;
use crate::activitypub::objects::activities::delete;
use crate::activitypub::objects::activities::follow;
use crate::activitypub::objects::activities::like;
use crate::activitypub::objects::collections::{OrderedCollection, OrderedCollectionPage};
use crate::local::actors::LocalActorStub;
use crate::error::bad_request;
use crate::local::notes;
use crate::local::InternalResult;
use crate::database::Database;
use crate::external::{get_external_actor, get_external_actor_for_note};
use crate::gateway;

// TODO: The actions in this file need a better, exportable, error type

#[tracing::instrument]
pub async fn get<T: DeserializeOwned>(url: &Url, sender: &LocalActorStub) -> InternalResult<T> {
    gateway::get_from_ap(url, sender).await
}

#[tracing::instrument]
pub async fn get_outbox(url: &Url, sender: &LocalActorStub) -> InternalResult<OrderedCollection<String>> {
    gateway::get_from_ap(url, sender).await
}

#[tracing::instrument]
pub async fn get_outbox_page(url: &Url, sender: &LocalActorStub) -> InternalResult<OrderedCollectionPage> {
    gateway::get_from_ap(url, sender).await
}

#[tracing::instrument]
pub async fn follow(db: Database, sender: LocalActorStub, actor_id: &str) -> InternalResult<()> {
    let ext_actor = get_external_actor(db.clone_conn()?, &sender, actor_id).await?;
    let inbox_url = ext_actor.inbox.parse().map_err(|_| bad_request("Invalid actor URI provided"))?;
    follow::send(db, sender, &ext_actor.id, inbox_url).await
}

#[tracing::instrument]
pub async fn unfollow(db: Database, sender: LocalActorStub, actor_id: &str) -> InternalResult<()> {
    follow::undo_send(db, sender, actor_id).await
}

#[tracing::instrument]
pub fn delete_note(db: &Database, sender: LocalActorStub, note_id: String) -> InternalResult<()> {
    delete::send_all(db, sender, note_id)
}

#[tracing::instrument]
pub async fn like(db: Database, sender: LocalActorStub, note_id: &str) -> InternalResult<()> {
    let ext_actor = get_external_actor_for_note(db.clone_conn()?, &sender, note_id).await?;
    let inbox_url = ext_actor.inbox.parse().map_err(|_| bad_request("Invalid actor URI provided"))?;
    like::send(db, sender, note_id, inbox_url).await
}

#[tracing::instrument(fields(note_id))]
pub fn publish_note(db: Database, actor_id: &str, content: &str, in_reply_to: Option<String>) -> InternalResult<String> {
    notes::create_and_federate(&db, actor_id, content, in_reply_to)
        .inspect(|note_id| { tracing::Span::current().record("note_id", note_id); })
        .inspect_err(|e| { tracing::error!("{e:?}"); })
}

// pub fn unlike(db: &Database, note_id: &str, profile: InternalActor) -> InternalResult<()> {
//     let activity_id: String = db.query_row(
//         "SELECT activity_id FROM likes WHERE profile_id = ?1 AND note_id = ?2",
//         (&profile.profile_id, note_id),
//         |row| row.get(0)
//         )?;
// }

