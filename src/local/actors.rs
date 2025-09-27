use openssl::pkey::{PKey, Private};
use serde::{Deserialize, Serialize};
use http::Uri;
use url::Url;
use rusqlite::Error::QueryReturnedNoRows;
use uuid::Uuid;
use crate::local::notes::get_notes_by_actor_id;
use crate::activitypub::{AtContext, Context};
use crate::activitypub::objects::activities::create::CreateActivity;
use crate::activitypub::objects::collections::{OrderedCollection, OrderedCollectionPage, OutboxPage, PageOrLink};
use crate::database::Database;
use crate::{get_server_internal_actor, query_row, query_row_custom};
use crate::activitypub::objects::actor::{Actor, ActorType, Icon, PublicKey};
use crate::activitypub::signature::generate_pkey_string;
use crate::server::ap_request::UnverifiedApRequest;
use crate::error::{bad_request, MainsailError};
use crate::local::feeds::{Meta, Post, TimelineEntry};
use crate::local::{InternalResult, notes};
use crate::server::response;
use crate::server::response::ServerResult;

#[derive(Clone, Debug, Serialize)]
pub struct LocalActorStub {
    pub id: String,
    pub domain: String,
    #[serde(skip)] pub pkey: PKey<Private>,
}

impl LocalActorStub {
    pub fn get(db: &Database, id: &str) -> Option<Self> {
        let domain = db.query_row(
            "SELECT value FROM globals WHERE key = 'domain'",
            [],
            |row| row.get(0),
        ).ok()?;

        let private_key_pem: String = db.query_row(
            "SELECT private_key_pem FROM actors WHERE id = ?1",
            [&id],
            |row| row.get(0),
        ).ok()?;
        let pkey = PKey::private_key_from_pem(private_key_pem.as_bytes()).ok()?;

        let current_profile = LocalActorStub { id: id.to_owned(), pkey, domain };
        Some(current_profile)
    }
}

pub fn create_local_actor(db: &Database, preferred_username: &str, display_name: &str, nickname: &str) -> InternalResult<LocalActorStub> {
    let domain: String = db.query_row("SELECT value FROM globals WHERE key = 'domain'", (), |row| row.get(0))?;
    let pkey = generate_pkey_string()?;
    let uuid = Uuid::new_v4().to_string();
    let id = id(&domain, &uuid);

    db.execute(
        "INSERT INTO actors (id, uuid, preferred_username, display_name, nickname, private_key_pem)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        (&id, uuid, preferred_username, display_name, nickname, &pkey),
    )?;

    LocalActorStub::get(db, &id)
        .ok_or(MainsailError::InternalServerError(format!("Failed to get actor {id} after creating it")))
}

pub fn create_local_actor_with_uuid(db: &Database, uuid: &str, preferred_username: &str, display_name: &str, nickname: &str) -> InternalResult<String> {
    let domain: String = db.query_row("SELECT value FROM globals WHERE key = 'domain'", (), |row| row.get(0))?;
    let pkey = generate_pkey_string()?;
    let id = id(&domain, uuid);

    db.execute(
        "INSERT INTO actors (id, uuid, preferred_username, display_name, nickname, private_key_pem)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        (&id, uuid, preferred_username, display_name, nickname, &pkey),
    )?;

    Ok(id)
}

fn id(domain: &str, uuid: &str) -> String {
    format!("https://{domain}/_mainsail/actors/{uuid}")
}

fn url(domain: &str, preferred_username: &str) -> String {
    format!("https://{domain}/@{preferred_username}")
}

pub fn inbox_url(domain: &str) -> String {
    format!("https://{domain}/inbox")
}

fn outbox_url(domain: &str, preferred_username: &str) -> String {
    format!("https://{domain}/@{preferred_username}/outbox")
}

fn following_url(domain: &str, preferred_username: &str) -> String {
    format!("https://{domain}/@{preferred_username}/following")
}

fn followers_url(domain: &str, preferred_username: &str) -> String {
    format!("https://{domain}/@{preferred_username}/followers")
}

pub fn get_outbox(db: &Database, actor_id: &str) -> InternalResult<OrderedCollection<String>> {
    let actor = get_actor_by_id(db, actor_id)?;
    let profile = query_row_custom!(
        db,
        Profile { total_items: i64 },
        "SELECT count(*) as total_items FROM notes WHERE actor_id = ?",
        [actor_id]
    )?;
    let total_items = profile.total_items as u64;

    // TODO: Pagination
    let first_page = format!("{}?page=1", actor.outbox);
    let last_page = format!("{}?page=1", actor.outbox);

    let outbox = OrderedCollection::new(actor.outbox, first_page, last_page, total_items);

    Ok(outbox)
}

pub fn get_outbox_page(db: &Database, actor_id: &str, _page_num: usize) -> InternalResult<OutboxPage> {
    let actor = get_actor_by_id(db, actor_id)?;
    let notes = get_notes_by_actor_id(db, actor_id)?;

    let page_url = format!("{}?page=1", actor.outbox);
    let items: Vec<CreateActivity> = notes.into_iter()
        .map(|internal_note| { internal_note.into_note() })
        .map(|note| { note.into_create() })
        .collect();

    let page = OrderedCollectionPage {
        context: AtContext::Context(Context::ActivityStreams),
        next: None,
        prev: Some(Box::new(PageOrLink::Link(page_url.to_owned()))),
        ordered_items: items,
    };

    Ok(page)
}

pub fn get_actor_id_from_uuid(db: &Database, uuid: &str) -> InternalResult<Option<String>> {
    let actor = query_row!(
        db,
        Profile { id: String },
        "FROM actors WHERE uuid = ?1",
        [uuid]
        );

    if let Err(QueryReturnedNoRows) = actor {
        return Ok(None);
    }

    Ok(Some(actor?.id))
}

pub fn get_profile_id_from_url(db: &Database, url: &str) -> InternalResult<Option<String>> {
    let uri: Uri = url.parse().map_err(|_| bad_request("Invalid URI provided"))?;
    let profile_id = uri.path().split('/').next_back();
    let profile = query_row!(
        db,
        Profile { uuid: String },
        "FROM actors WHERE uuid = ?1",
        [profile_id]
        );

    if let Err(QueryReturnedNoRows) = profile {
        return Ok(None);
    }

    Ok(Some(profile?.uuid))
}

// TODO this is in the wrong place
pub async fn get_followers(req: UnverifiedApRequest, actor: Actor) -> ServerResult {
    let req = req.authenticate().await?;
    let mut query = req.db.prepare("SELECT sender_id FROM followers WHERE actor_id = ?1")?;

    let followers = query.query_map([actor.id], |row| row.get(0))?;
    let followers: Vec<String> = followers.collect::<Result<_, _>>()?;

    let url = followers_url(&req.domain, &actor.preferred_username);
    let collection = OrderedCollection::new_from_list(url, followers);
    let body = serde_json::json!(collection).to_string();
    response::send(body)
}

// TODO this is in the wrong place
pub async fn get_following(req: UnverifiedApRequest, actor: Actor) -> ServerResult {
    let req = req.authenticate().await?;

    let mut query = req.db.prepare("SELECT followed_actor_id FROM following WHERE actor_id = ?1")?;
    let following = query.query_map([actor.id], |row| row.get(0))?;
    let following: Vec<String> = following.collect::<Result<_, _>>()?;

    let url = following_url(&req.domain, &actor.preferred_username);
    let collection = OrderedCollection::new_from_list(url, following);
    let body = serde_json::json!(collection).to_string();
    response::send(body)
}

pub fn get_server_id(domain: &str) -> String {
    format!("https://{domain}/_mainsail/server-actor")
}

pub fn get_server_actor(db: &Database) -> InternalResult<Actor> {
    let server = get_server_internal_actor(db)?;
    let domain = &server.domain;

    // TODO yes I know this is a huge waste
    let pkey = server.pkey.private_key_to_pem_pkcs8()?;
    let private_key_pem = String::from_utf8(pkey).map_err(|e| MainsailError::GenericError(e.into()))?;

    let id = get_server_id(domain);

    let public_key = PublicKey::new(&id, &private_key_pem);

    let context = vec![Context::ActivityStreams, Context::SecurityV1];
    let context = AtContext::Collection(context);
    let actor = Actor {
        context: Some(context),
        id: id.to_owned(),
        url: id.to_owned(),
        name: None,
        actor_type: ActorType::Person,
        summary: Some("This is a server actor for a Mainsail-based server".to_owned()),
        preferred_username: "MainsailInternal".to_owned(),
        icon: None,
        inbox: id.clone(),
        outbox: id.clone(),
        followers: None,
        following: None,
        public_key,
    };

    Ok(actor)
}

pub fn get_actor_notes(db: &Database, actor_id: &str) -> InternalResult<Vec<TimelineEntry>> {
    let actor = get_actor_by_id(db, actor_id)?;
    let notes = notes::get_notes_by_actor_id(db, &actor.id)?;

    // TODO: do this without cloning the actor, maybe?
    let items = notes.into_iter().map(|note| {
        let meta = Meta { internal_id: note.uuid };
        let item = Post::new(note.note, actor.to_owned(), meta);
        TimelineEntry::Post(item.into())
    });

    Ok(items.collect())
}

enum ActorColumnParameter {
    Id,
    Uuid,
    PreferredUsername,
}

impl ActorColumnParameter {
    pub fn as_str(&self) -> &str {
        match self {
            ActorColumnParameter::Id => "id",
            ActorColumnParameter::Uuid => "uuid",
            ActorColumnParameter::PreferredUsername => "preferred_username"
        }
    }
}

// TODO these should all probably be result-options
pub fn get_actor_by_id(db: &Database, id: &str) -> InternalResult<Actor> {
    get_actor_internal(db, ActorColumnParameter::Id, id)
}

pub fn get_actor_by_uuid(db: &Database, uuid: &str) -> InternalResult<Actor> {
    get_actor_internal(db,ActorColumnParameter::Uuid, uuid)
}

pub fn get_actor_by_preferred_username(db: &Database, preferred_username: &str) -> Option<Actor> {
    log::debug!("Getting actor by preferred_username: {preferred_username}");
    get_actor_internal(db, ActorColumnParameter::PreferredUsername, preferred_username).ok()
}


fn get_actor_internal(db: &Database, param: ActorColumnParameter, value: &str) -> InternalResult<Actor> {
    #[derive(Serialize, Deserialize)]
    struct DbActor {
        id: String,
        uuid: String,
        preferred_username: String,
        display_name: String,
        icon_url: Option<String>,
        nickname: String,
        following_count: i64,
        follower_count: i64,
        private_key_pem: String,
    }

    let parameter = param.as_str();
    let actor = db.query_row(
        &format!("
                SELECT
                    id,
                    uuid,
                    preferred_username,
                    display_name,
                    icon_url,
                    nickname,
                    (SELECT count(*) FROM following WHERE actor_id = id) as following_count,
                    (SELECT count(*) FROM followers WHERE actor_id = id) as follower_count,
                    private_key_pem
                FROM actors
                WHERE {parameter} = ?2"),
        [&parameter, &value],
        |row| {
            let profile = DbActor {
                id: row.get("id")?,
                uuid: row.get("uuid")?,
                preferred_username: row.get("preferred_username")?,
                display_name: row.get("display_name")?,
                icon_url: row.get("icon_url")?,
                nickname: row.get("nickname")?,
                following_count: row.get("following_count")?,
                follower_count: row.get("follower_count")?,
                private_key_pem: row.get("private_key_pem")?,
            };
            Ok(profile)
        });
    let actor = match actor {
        Ok(x) => x,
        Err(QueryReturnedNoRows) => return Err(MainsailError::NotFound),
        Err(e) => return Err(e.into())
    };

    let id = actor.id.parse::<Url>().map_err(|_| {
        MainsailError::InvalidActorUri(format!("Failed to parse {} as URL", actor.id))
    })?;
    let domain = id.domain().ok_or_else(|| {
        MainsailError::InvalidActorUri(format!("No domain found for {id}"))
    })?;

    let inbox = inbox_url(domain);
    let outbox = outbox_url(domain, &actor.preferred_username);
    let following = following_url(domain, &actor.preferred_username);
    let followers = followers_url(domain, &actor.preferred_username);
    let public_key = PublicKey::new(&actor.id, &actor.private_key_pem);

    let icon = actor.icon_url.map(Icon::new_from_url);

    let context = vec![Context::ActivityStreams, Context::SecurityV1];
    let context = AtContext::Collection(context);
    let actor = Actor {
        context: Some(context),
        id: actor.id,
        url: url(domain, &actor.preferred_username),
        name: Some(actor.display_name),
        actor_type: ActorType::Person,
        summary: None, // TODO add back summary
        preferred_username: actor.preferred_username,
        icon,
        inbox,
        outbox,
        followers: Some(followers),
        following: Some(following),
        public_key,
    };

    Ok(actor)
}

