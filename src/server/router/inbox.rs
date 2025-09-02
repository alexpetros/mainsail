use serde::Deserialize;
use crate::activitypub::objects::activities::update::{self, UpdateActivity};
use crate::activitypub::objects::activities::{accept, announce, create, delete, follow, like};
use crate::activitypub::objects::activities::accept::AcceptActivity;
use crate::activitypub::objects::activities::announce::AnnounceActivity;
use crate::activitypub::objects::activities::create::CreateActivity;
use crate::activitypub::objects::activities::delete::DeleteActivity;
use crate::activitypub::objects::activities::follow::FollowActivity;
use crate::activitypub::objects::activities::GenericActivity;
use crate::activitypub::objects::activities::like::LikeActivity;
use crate::activitypub::objects::activities::undo::UndoActivity;
use crate::activitypub::objects::actor::Actor;
use crate::server::ap_request::{AuthedApRequest, UnverifiedApRequest};
use crate::error::{bad_request, unauthorized, MainsailError};
use crate::server::response;
use crate::server::response::ServerResult;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum InboxPost {
    Accept(AcceptActivity<serde_json::Value>),
    Delete(DeleteActivity),
    Create(CreateActivity),
    UndoCreate(UndoActivity<CreateActivity>),
    Like(LikeActivity),
    UndoLike(UndoActivity<LikeActivity>),
    Announcement(AnnounceActivity),
    UndoAnnouncement(UndoActivity<AnnounceActivity>),
    Follow(FollowActivity),
    UndoFollow(UndoActivity<FollowActivity>),
    UpdatePerson(UpdateActivity<Actor>),
    UnknownActivity(GenericActivity),
}

pub async fn post(req: UnverifiedApRequest) -> ServerResult {
    let req = req.authenticate().await?;
    receive(req).await
}

#[tracing::instrument(name = "inbox", skip(req), fields(actor, activity_id, activity_type))]
async fn receive(req: AuthedApRequest) -> ServerResult {
    let text = req.text()?;
    let body: InboxPost = req.json()?;

    // I feel a little silly parsing this twice, but also, it's probably fine for now?
    // TODO: Make this an enum with Note
    // See: https://www.w3.org/TR/activitypub/#object-without-create
    let activity: GenericActivity = req.json()?;
    // verify_cached_actor(req.db.get_path() activity.actor);

    let span = tracing::Span::current();
    span.record("actor", &activity.actor)
        .record("activity_id", activity.id.as_str())
        .record("activity_type", &activity._type);

    if activity.actor != req.actor.id {
        let msg = format!("Requestor {} does not match activity: {}", activity.actor, req.actor.id);
        return Err(unauthorized(msg));
    }

    // I feel VERY silly doing this block
    // Basically this ensures that the statement is dropped before we move onto the async bits
    // Not sure why drop(stmt) doesn't accomplish this though...
    // (I do not really understand async rust)
    {
        let mut stmt = req.db.prepare("INSERT OR IGNORE INTO received_activities (activity_id, actor_id, object) VALUES (?1, ?2, ?3)")?;
        let _ = stmt.execute([activity.id.as_str(), activity.actor.as_str(), &text])?;
        tracing::debug!("Inserted activity into database")
    }

    // Delete saved activity if it's an undo activity
    // TODO verify that the person doing the undoing is the person who did the activity
    if activity._type == "Undo" {
        let id_to_delete = activity.object.get("id")
            .ok_or(bad_request("Missing ID to undo"))?
            .as_str()
            .ok_or(bad_request("Couldn't parse ID as string"))?;

        let mut stmt = req.db.prepare(
            "DELETE FROM received_activities WHERE activity_id = ?1 AND actor_id = ?2")?;
        let num_changes = stmt.execute([id_to_delete, &req.actor.id])?;

        // Don't delete the notification unless we deleted a verified activity
        // The reason we don't just return immediately is because it's plausible that the
        // received_activity got purged. This maybe have been a design mistake, but I don't think
        // we're going to save every received_activity of durable significance forever.
        if num_changes == 0 {
            tracing::warn!("Received request to undo missing activity {id_to_delete}")
        } else {
            let mut stmt = req.db.prepare("DELETE FROM notifications WHERE activity_id = ?1")?;
            let _ = stmt.execute([id_to_delete]);
        }
    }

    tracing::debug!("sending activity {} to {} handler", activity.id, activity._type);
    match body {
        InboxPost::Follow(activity) => follow::receive(req, activity).await,
        InboxPost::UndoFollow(activity) => follow::undo_receive(req, activity),
        InboxPost::Create(activity) => create::receive(req, activity).await,
        InboxPost::UndoCreate(activity) => create::undo_receive(req, activity),
        InboxPost::Accept(activity) => accept::receive(req, activity),
        InboxPost::Delete(activity) => delete::receive(req, activity),
        InboxPost::Like(activity) => like::receive(req, activity),
        InboxPost::UndoLike(activity) => like::undo_receive(req, activity),
        InboxPost::Announcement(activity) => announce::receive(req, activity),
        InboxPost::UndoAnnouncement(activity) => announce::undo_receieve(req, activity),
        InboxPost::UpdatePerson(activity) => update::receive(req, activity),
        InboxPost::UnknownActivity(activity) => log_unknown(req, activity)
    }
}

fn log_unknown(req: AuthedApRequest, activity: GenericActivity) -> ServerResult {
    let object = serde_json::to_string(&activity.object)
        .map_err(|_| { MainsailError::InternalSerializationError })?;
    let _ = req.db.execute("
        INSERT OR IGNORE INTO received_activities (activity_id, actor_id, object)
        VALUES (?1, ?2?, ?3)",
                           (activity.id.as_str(), activity.actor, object));
    response::not_implemented()
}

#[cfg(test)]
mod tests {
    use http::StatusCode;

    use crate::activitypub::objects::activities::like::LikeActivity;
    use crate::activitypub::objects::activities::Activity;
    use crate::tests::data::EXT_ACTOR_1;
    use crate::tests::data::EXT_ACTOR_2;
    use crate::tests::get_memory_db;
    use crate::tests::requests::create_authed_inbox_post;

    const ACTIVITY_ID_1: &str = "https://example.com/activity/1";
    const OBJECT_REAL: &str = "https://example.com/notes/1";
    

    #[tokio::test]
    async fn inbox_post_authed_actor_doesnt_match_activity() {
        let (_db, name) = get_memory_db();
        let like_activity = LikeActivity::new_external(ACTIVITY_ID_1, &EXT_ACTOR_2.id, OBJECT_REAL);
        let req = create_authed_inbox_post(like_activity.json(), &name, &EXT_ACTOR_1);
        let res = super::receive(req).await.unwrap_err();

        assert_eq!(res.status(), StatusCode::UNAUTHORIZED)
    }
}
