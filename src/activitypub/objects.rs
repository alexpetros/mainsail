//! ActivityPub [Objects](https://www.w3.org/TR/activitystreams-vocabulary/#object-types)
use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::activitypub::objects::note::Note;

pub mod note;
pub mod tags;
pub mod activities;
pub mod collections;
pub mod actor;

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum Object {
    Note(Note),
    #[serde(untagged)]
    Unknown(Value),
}
