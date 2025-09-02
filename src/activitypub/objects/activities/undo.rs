//! ActivityPub [Undo Activity](https://www.w3.org/TR/activitystreams-vocabulary/#dfn-undo)
use serde::{Deserialize, Serialize};
use url::Url;
use uuid::Uuid;
use crate::activitypub::Context;
use crate::activitypub::AtContext;
use crate::local::actors::LocalActorStub;
use crate::gateway::send_activity;
use crate::local::InternalResult;
use crate::database::Database;

use super::Activity;

#[derive(Debug, Serialize, Deserialize)]
enum UndoActivityType { Undo }

#[derive(Debug, Serialize, Deserialize)]
pub struct UndoActivity<T> {
    #[serde(rename = "type")]
    activity_type: UndoActivityType,
    #[serde(rename = "@context")]
    pub context: Option<AtContext>,
    pub id: Url,
    pub actor: String,
    pub object: T,
}

impl<T: Serialize> UndoActivity<T> {
    pub fn new(activity_id: String, sender_id: String, object: T) -> UndoActivity<T> {
        UndoActivity {
            context: Some(AtContext::Context(Context::ActivityStreams)),
            activity_type: UndoActivityType::Undo,
            id: activity_id.parse().unwrap(),
            actor: sender_id,
            object,
        }
    }
}

impl<T: Serialize> Activity for UndoActivity<T> {
    fn get_id(&self) -> &Url {
        &self.id
    }

    fn json(&self) -> String {
        serde_json::json!(self).to_string()
    }
}

pub async fn send(db: Database, sender: LocalActorStub, recipient_inbox_url: Url, activity: &impl Activity) -> InternalResult<()> {
    let activity_id = Uuid::new_v4().to_string();
    let activity_id = format!("https://{}/activity/{}", sender.domain, activity_id);
    let undo_activity = UndoActivity::new(activity_id, sender.id.clone(), &activity);
    send_activity(db, recipient_inbox_url, sender, &undo_activity).await
}

// Receiving an UndoActivity is handled by handlers in inbox.rs
// I think it would be nice to have that match statement here instead
// but I would have to figure out if there's a way for serde to parse an `impl Activity`
// which I haven't figured out how to do
