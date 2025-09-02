use serde::{Deserialize, Serialize};
use url::Url;
use crate::activitypub::{ApUrl, AtContext};

pub mod accept;
pub mod like;
pub mod undo;
pub mod delete;
pub mod follow;
pub mod create;
pub mod announce;
pub mod update;

pub trait Activity: Serialize {
    fn get_id(&self) -> &Url;
    fn json(&self) -> String;
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GenericActivity {
    #[serde(rename = "@context")]
    pub context: Option<AtContext>,
    pub id: Url,
    #[serde(rename = "type")]
    pub _type: String,
    pub actor: ApUrl,
    pub object: serde_json::Value,
}

impl Activity for GenericActivity {
    fn get_id(&self) -> &Url {
        &self.id
    }

    fn json(&self) -> String {
        serde_json::json!(self).to_string()
    }
}
