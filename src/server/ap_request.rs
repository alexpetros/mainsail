use std::ops::Deref;

use hyper::body::Bytes;
use http::Request;
use serde::Deserialize;
use tracing::warn;

use crate::activitypub::objects::actor::Actor;
use crate::activitypub::signature::validate_activitypub_request;
use crate::error::{bad_request, MainsailError};
use crate::local::InternalResult;
use crate::database::Database;

use crate::get_server_internal_actor;

pub struct NoAuth;

pub type UnverifiedApRequest = ApRequest<NoAuth>;
pub type AuthedApRequest = ApRequest<Actor>;

pub struct ApRequest<T> {
    pub request: Request<Bytes>,
    pub db: Database,
    pub domain: String,
    pub actor: T,
}

pub async fn new_ap_request(
    request: Request<Bytes>,
    db: Database,
    domain: String,
) -> Result<UnverifiedApRequest, MainsailError> {
    Ok(ApRequest { request, db, domain, actor: NoAuth })
}

impl<T> ApRequest<T> {
    pub fn get_url_param(&self, pos: usize, message: &str) -> Result<&str, MainsailError> {
        self.uri().path().split('/').nth(pos)
            .ok_or(bad_request(message))
    }

    pub fn get_int_url_param(&self, pos: usize, message: &str) -> Result<i64, MainsailError> {
        let str_param = self.get_url_param(pos, message)?;
        let param = str_param.parse::<i64>();
        match param {
            Ok(p) => Ok(p),
            Err(_) => {
                warn!("Failed to parse parameter {} as integer in URL {}", str_param, self.uri());
                Err(bad_request(message))
            }
        }
    }

    pub fn get_trailing_param(&self, message: &str) -> Result<&str, MainsailError> {
        self.uri()
            .path()
            .split('/')
            .next_back()
            .ok_or(bad_request(message))
    }

    pub fn json<'a, V: Deserialize<'a>>(&'a self) -> InternalResult<V> {
        serde_json::from_slice::<V>(self.body()).map_err(|_| {
            bad_request("Sorry, we were unable to parse that request. \
                         Please check that it contains all the required fields."
            )
        })
    }

    pub fn text(&self) -> Result<String, MainsailError> {
        String::from_utf8(self.body().to_vec()).map_err(|_| { MainsailError::BodyNotUTF8 })
    }
}

impl ApRequest<NoAuth> {
    pub async fn authenticate(self) -> InternalResult<AuthedApRequest> {
        let server = get_server_internal_actor(&self.db)?;
        let actor = validate_activitypub_request(&self, &server).await?;

        let request = self.request;
        let db = self.db;
        let domain = self.domain;
        let data = actor;

        Ok(ApRequest { request, db, domain, actor: data })
    }
}

impl<T> Deref for ApRequest<T> {
    type Target = Request<Bytes>;
    fn deref(&self) -> &Self::Target {
        &self.request
    }
}
