use url::Url;
use crate::activitypub::objects::activities::announce::AnnounceActivity;
use crate::activitypub::objects::activities::follow::FollowActivity;
use crate::activitypub::objects::activities::like::LikeActivity;
use crate::activitypub::objects::note::Note;
use crate::error;
use crate::local::InternalResult;
use crate::database::Database;

use super::notes;
use super::notes::get_actor_id_for_note;

fn insert_notification(db: &Database, actor_id: &str, activity_id: &str) -> InternalResult<i64> {
    db.execute(
        "INSERT INTO notifications (actor_id, activity_id) VALUES (?1, ?2)",
        (actor_id, &activity_id),
    )?;
    Ok(db.last_insert_rowid())
}

fn get_actor_for_note_or_return_404(db: &Database, note_id: &str) -> InternalResult<String> {
    let actor_id = get_actor_id_for_note(db, note_id)?;
    match actor_id {
        Some(id) => Ok(id),
        None => {
            tracing::warn!("Attempted to like unknown note {note_id}");
            Err(error::not_found())
        }
    }
}

pub fn get_unread_notification_count(db: &Database, actor_id: &str) -> i64 {
    let count: i64 = db.query_row(
        "SELECT count(*) FROM notifications WHERE actor_id = ?1 AND is_read = FALSE",
        [actor_id],
        |row| { row.get(0) },
    ).unwrap_or_else(|e| {
        // We don't want this to be fallable; for now it's fine to ignore it if this fails
        tracing::warn!("Error getting notification count: {}", e);
        0
    });

    count
}

pub fn insert_follow_notification(db: &Database, actor_id: &str, activity: &FollowActivity) -> InternalResult<()> {
    let notification_id = insert_notification(db, actor_id, activity.id.as_str())?;

    db.execute(
        "INSERT OR REPLACE INTO notifications_follows (notification_id, actor_id)
        VALUES (?1, ?2)",
        (notification_id, &activity.actor))?;
    Ok(())
}

pub fn insert_like_notification(db: &Database, note_id: &str, activity: &LikeActivity) -> InternalResult<()> {
    let actor_id = get_actor_for_note_or_return_404(db, note_id)?;
    let notification_id = insert_notification(db, &actor_id, activity.id.as_str())?;
    db.execute(
        "INSERT INTO notifications_likes (notification_id, note_id, actor_id)
        VALUES (?1, ?2, ?3)",
        (notification_id, note_id, &activity.actor))?;
    Ok(())
}

pub fn insert_announce_notification(db: &Database, note_id: &str, activity: &AnnounceActivity) -> InternalResult<()> {
    let actor_id = get_actor_for_note_or_return_404(db, note_id)?;
    let notification_id = insert_notification(db, &actor_id, activity.id.as_str())?;
    db.execute(
        "INSERT INTO notifications_announcements (notification_id, note_id, actor_id)
        VALUES (?1, ?2, ?3)",
        (notification_id, note_id, &activity.actor))?;

    Ok(())
}

pub fn insert_reply_notification(db: &Database, note: &Note, activity_id: &Url) -> InternalResult<()> {
    let in_reply_to = match &note.in_reply_to {
        Some(id) => id,
        None => return Ok(())
    };
    let Some(replied_to_note) = notes::get_note(db, in_reply_to)? else {
        return Ok(())
    };

    let notification_id = insert_notification(db, &replied_to_note.note.attributed_to, activity_id.as_str())?;

    db.execute(
        "INSERT INTO notifications_replies (notification_id, note_id, replied_to_note_id, actor_id)
        VALUES (?1, ?2, ?3, ?4)",
        (notification_id, &note.id, &replied_to_note.note.id, &note.attributed_to))?;
    Ok(())
}
