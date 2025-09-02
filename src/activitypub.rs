//! Representations of activitypub concepts.
//!
//! The objects here are intended to correspond as faithfully as possible with the activitypub
//! spec, with a couple exceptions. Generally speaking, if we receive an object that we're unable
//! to parse, it requires an update here.

use std::option::Option;

use serde::{Deserialize, Serialize};

pub mod webfinger;
pub mod objects;
pub mod signature;
pub mod actions;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Context {
    #[serde(rename = "https://www.w3.org/ns/activitystreams")]
    ActivityStreams,
    #[serde(rename = "https://w3id.org/security/v1")]
    SecurityV1,
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

pub const BASIC_CONTEXT: Option<AtContext> = Some(AtContext::Context(Context::ActivityStreams));
// pub const SECURITY_CONTEXT: Option<AtContext> = Some(AtContext::Context(Context::ActivityStreams));

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum AtContext {
    Context(Context),
    Collection(Vec<Context>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum UrlOrObject<T> {
    Url(String),
    Object(T),
}

// This is to denote strings that should be URLs
type ApUrl = String;
