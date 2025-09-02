use std::sync::Arc;
use hyper::server::conn::http1;
use hyper::{Method, Request};
use http::header::{ACCEPT, CONTENT_TYPE};
use hyper::body::{Bytes, Incoming};
use hyper_util::rt::TokioIo;
use hyper_util::service::TowerToHyperService;
use local::actors::LocalActorStub;
use openssl::pkey::PKey;
use activitypub::objects::actor::FullHandle;
use crate::error::{bad_request, MainsailError};
use local::InternalResult;
use crate::database::Database;
use server::media_type::MediaType;
use tower::{Layer, Service};
use http_body::Body;
use std::error::Error;
use http::Response;
use http_body_util::combinators::BoxBody;
use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};
use crate::server::response::HyperResponse;

#[cfg(test)]
mod tests;

mod server;
mod gateway;
mod utils;

pub mod activitypub;
pub mod local;
pub mod database;
pub mod error;
pub mod external;

static PUBLIC_STREAM: &str = "https://www.w3.org/ns/activitystreams#Public";

static ACTIVITYPUB_MEDIA_TYPES: [MediaType<'_>; 3] = [
    MediaType { media_type: "application", media_subtype: "activity+json" },
    MediaType { media_type: "application", media_subtype: "ld+json" },
    MediaType { media_type: "application", media_subtype: "json" },
];

pub async fn handle_request(req: Request<Incoming>, db_path: Arc<String>) -> HyperResponse {
    server::serve(req, db_path).await.unwrap()
}

pub fn get_full_handle(username: &str) -> Result<FullHandle, MainsailError> {
    let mut splits = username.splitn(3, '@').peekable();
    splits.next_if_eq(&"");
    let preferred_username = splits
        .next()
        .ok_or(bad_request("Missing user name"))?
        .trim()
        .to_owned();
    let host = splits
        .next()
        .ok_or(bad_request("Missing host name"))?
        .trim()
        .to_owned();

    Ok(FullHandle {
        preferred_username,
        host,
    })
}

// Check whether the header is asking for (ActivityPub) JSON
pub fn is_ap_req<T>(req: &Request<T>) -> bool {
    // Use the "accept" header for GET requests, "content-type" for the others
    let header_name = if req.method() == Method::GET { ACCEPT } else { CONTENT_TYPE };
    let accept_header_value = req.headers().get(header_name).and_then(|s| { s.to_str().ok() });

    let accepted_types = match accept_header_value {
        None => { return false }
        Some(t) => { MediaType::get_list(t) }
    };

    // Return ActivityPub JSON if it's one of the types that was requested
    // Technically I'm probably supposed to be parsing the q values but, if you wanted JSON,
    // why would you send `application/activity+json;q=0.5, text/html;q=1`
    accepted_types.iter().any(|t| ACTIVITYPUB_MEDIA_TYPES.contains(t))
}

pub fn get_server_internal_actor(db: &Database) -> InternalResult<LocalActorStub> {
    let private_key_pem: String = db.query_row(
        "SELECT value FROM globals WHERE key = 'server_pkey'",
        [],
        |row| row.get(0),
    )?;
    let pkey = PKey::private_key_from_pem(private_key_pem.as_bytes())?;
    let domain: String = db.query_row(
        "SELECT value FROM globals WHERE key = 'domain'",
        [],
        |row| row.get(0),
    )?;

    let server = LocalActorStub { id: "-1".to_string(), domain, pkey };
    Ok(server)
}

// Doing this because MacOS shows me an annoying notification for the latter
const HOST: &str = if cfg!(debug_assertions) {
    "127.0.0.1"
} else {
    "0.0.0.0"
};

type ServerResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

pub async fn run_server(db_path: String, port: Option<u16>) -> ServerResult {
    let port = port.unwrap_or(0);
    let addr: core::net::SocketAddr = format!("{HOST}:{port}").parse()?;

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Now listening at http://localhost:{}", port);



    loop {
        let (stream, _) = listener.accept().await?;
        // Wrapper to use Hyper traits with Tokio streams
        let io = TokioIo::new(stream);
        let service = MainsailService::new(db_path.clone());
        let service = TowerToHyperService::new(service);

        // Spawn a tokio task to serve multiple connections concurrently
        tokio::spawn(async move {
            if let Err(err) = http1::Builder::new().serve_connection(io, service).await {
                tracing::error!("Error serving connection: {}", err);
            }
        });
    }
}

#[derive(Clone)]
pub struct MainsailService {
    db_path: Arc<String>
}

impl MainsailService {
    pub fn new(db_path: String) -> Self {
        Self {
            db_path: Arc::new(db_path)
        }
    }
}

impl<B> Service<Request<B>> for MainsailService
where
    B: Body<Data = Bytes> + Send + 'static,
    B::Error: Into<Box<dyn Error + Send + Sync>>,
{
    type Response = Response<BoxBody<Bytes, Infallible>>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<B>) -> Self::Future {
        let res = server::serve(req, self.db_path.clone());
        Box::pin(res)
    }
}

#[derive(Clone)]
pub struct MainsailLayer {
    db_path: String
}

impl MainsailLayer {
    pub fn new(db_path: String) -> Self {
        Self { db_path }
    }
}

impl<S> Layer<S> for MainsailLayer {
    type Service = MainsailService;

    fn layer(&self, _inner: S) -> Self::Service {
        MainsailService::new(self.db_path.clone())
    }
}