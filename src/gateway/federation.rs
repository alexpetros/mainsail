use serde_json::json;
use crate::local::notes::InternalNote;
use crate::gateway::post_to_ap;
use crate::local::actors::LocalActorStub;
use crate::local::InternalResult;
use crate::database::Database;
use crate::activitypub::objects::activities::Activity;

fn get_follower_inboxes(db: &Database, sender: &LocalActorStub) -> InternalResult<Vec<String>> {
    let mut query = db.prepare(
        "
        SELECT sender_id, object ->> 'inbox' as inbox, type
        FROM followers
        LEFT JOIN cache ON followers.sender_id = cache.actor
        WHERE followers.actor_id = ?1 AND cache.type = 'Person'
        ",
    )?;

    let inboxes = query.query_map(
        [&sender.id],
        |row| { row.get("inbox") },
    )?;

    let inboxes = inboxes.collect::<Result<_, _>>()?;

    Ok(inboxes)
}

/// Send a new note out to all subscribed inboxes
pub fn federate_note(db: &Database, sender: LocalActorStub, note: InternalNote) -> InternalResult<()> {
    let inboxes = get_follower_inboxes(db, &sender)?;
    tokio::spawn(async move {
        let create_activity = note.into_create();
        let body = json!(create_activity).to_string();
        federate(sender, body, inboxes).await;
    });

    Ok(())
}

/// Send delete to all subscribed inboxes
pub fn federate_activity(db: &Database, sender: LocalActorStub, activity: impl Activity) -> InternalResult<()> {
    // TODO add to sent activities?
    let inboxes = get_follower_inboxes(db, &sender)?;
    let body = activity.json();
    tokio::spawn(async move {
        federate(sender, body, inboxes).await;
    });

    Ok(())
}

/// Right now this is all done synchronously, which is potentially problematic
async fn federate(sender: LocalActorStub, body: String, inboxes: Vec<String>) {
    for inbox in inboxes {
        let body = body.to_owned();
        let inbox_url = match inbox.parse() {
            Ok(inbox_url) => inbox_url,
            Err(e) => {
                tracing::warn!("Unable to parse inbox \"{inbox}\": {e}");
                continue;
            }
        };

        tracing::info!("Federating to {} as {}", inbox, sender.id);
        let res = post_to_ap(&inbox_url, &sender, body).await;
        let res = match res {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Failed to send note to {inbox_url}: {e:?}");
                continue;
            }
        };

        let code = res.status();
        let res_body = res.text().await;
        let res_body = res_body.unwrap_or_else(|e| { e.to_string() });
        if code.as_u16() > 399 {
            tracing::warn!("Received {code} from {inbox_url} with body {res_body}");
        } else {
            tracing::debug!("Received {} {}", code, res_body);
        }
    }
}
