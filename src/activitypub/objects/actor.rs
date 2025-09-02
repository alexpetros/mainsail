//! ActivityPub [Actor Object](https://www.w3.org/TR/activitypub/#actor-objects)
use std::fmt::Display;

use openssl::pkey::PKey;
use rusqlite::ToSql;
use serde::{Deserialize, Serialize};

use crate::{external, gateway};
use crate::activitypub::AtContext;
use crate::database::Database;
use crate::error::{bad_gateway, MainsailError};
use crate::local::actors::LocalActorStub;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Actor {
    #[serde(rename = "@context")]
    pub context: Option<AtContext>,
    pub id: String,
    pub url: String,
    pub summary: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub actor_type: ActorType,
    #[serde(rename = "preferredUsername")]
    pub preferred_username: String, // This might be optional
    pub inbox: String,
    pub outbox: String,
    pub followers: Option<String>,
    pub following: Option<String>,
    #[serde(rename = "publicKey")]
    pub public_key: PublicKey,
    pub icon: Option<Icon>,
}

impl Actor {
    pub fn json(&self) -> String {
        serde_json::json!(self).to_string()
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum ActorType {
    Person,
    Application,
    Service,
}

impl ActorType {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Person => "Person",
            Self::Application => "Application",
            Self::Service => "Service",
        }
    }
}

impl ToSql for ActorType {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        Ok(self.as_str().into())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum Icon {
    #[serde(untagged)]
    Full(FullIcon),
    #[serde(untagged)]
    Single(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FullIcon {
    #[serde(rename = "type")]
    pub icon_type: String,
    #[serde(rename = "mediaType")]
    pub media_type: Option<String>,
    pub url: String,
}

impl Icon {
    pub fn local_jpg(domain: &str, path: &str) -> Icon {
        let icon = FullIcon {
            icon_type: "Image".to_owned(),
            media_type: Some("image/jpeg".to_owned()),
            url: format!("https://{domain}/{path}"),
        };
        Icon::Full(icon)
    }

    pub fn new_from_url(url: String) -> Icon {
        let file_type = url.rsplit_once(".")
            .map(|tup| tup.1.to_lowercase())
            .unwrap_or("".to_owned())
            ;

        let media_type = match file_type.as_str() {
            ".jpg" | ".jpeg" => Some("image/jpeg".to_owned()),
            "png" => Some("image/png".to_owned()),
            _ => Some("image/jpeg".to_owned())
        };

        let icon = FullIcon {
            icon_type: "Image".to_owned(),
            media_type,
            url,
        };
        Icon::Full(icon)
    }

    pub fn get_url(&self) -> &str {
        match self {
            Icon::Full(i) => &i.url,
            Icon::Single(i) => i
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PublicKey {
    pub id: String,
    pub owner: String,
    #[serde(rename = "publicKeyPem")]
    pub public_key_pem: String,
}

impl PublicKey {
    pub fn new(id: &str, private_key_pem: &str) -> PublicKey {
        let private_key_pem = PKey::private_key_from_pem(private_key_pem.as_bytes()).unwrap();
        let public_key_pem = private_key_pem.public_key_to_pem().unwrap();
        PublicKey {
            id: format!("{id}#main-key"),
            owner: id.to_owned(),
            public_key_pem: String::from_utf8(public_key_pem).unwrap().trim().to_owned(),
        }
    }
}

impl Display for PublicKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::json!(self))
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum LinkType {
    #[serde(rename = "application/activity+json")]
    ActivityJson,
    #[serde(untagged)]
    Unknown(serde_json::Value),
}

pub async fn search_for_actor(db: Database, handle: &FullHandle, current_profile: &LocalActorStub) -> Result<Option<Actor>, MainsailError> {
    let FullHandle { preferred_username, host } = handle;
    let web_finger = gateway::get_webfinger(host, preferred_username).await?;
    let actor_link = web_finger.links
        .as_ref()
        .and_then(|links| {
            links
                .iter()
                .find(|l| l.rel == "self" && l.link_type == Some(LinkType::ActivityJson))
        })
        .and_then(|link| link.href.clone())
        .ok_or(bad_gateway("Bad link from webfinger"))?;

    let actor = external::get_external_actor(db, current_profile, &actor_link).await?;
    Ok(Some(actor))
}

#[derive(Debug)]
pub struct FullHandle {
    pub preferred_username: String,
    pub host: String,
}

impl FullHandle {
    pub fn get_local_url(&self) -> String {
        format!("/feeds/@{}@{}", self.preferred_username, self.host)
    }
}

impl Display for FullHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "@{}@{}", self.preferred_username, self.host)
    }
}
