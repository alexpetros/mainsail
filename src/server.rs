use std::convert::Infallible;
use std::error::Error;
use std::sync::Arc;

use http::{Method, Request, Response, StatusCode};
use http_body::Body;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Bytes;
use tracing::Instrument;

use crate::database::Database;
use crate::server::response::{HyperResponse, MainsailResponse};

mod router;
pub mod response;
pub mod media_type;
pub mod ap_request;

const GET: &Method = &Method::GET;
const POST: &Method = &Method::POST;

pub async fn serve<B>(req: Request<B>, db_path: Arc<String>) -> Result<HyperResponse, Infallible>
where
    B: Body<Data = Bytes> + Send + 'static,
    B::Error: Into<Box<dyn Error + Send + Sync>>
{

    // TODO refactor this out
    let (parts, body) = req.into_parts();
    let body = Limited::new(body, 1024 * 64);
    let body = body.collect().await;
    let body = body.map(|r| r.to_bytes());
    let Ok(bytes) = body else {
        let body = Full::from("Something went wrong")
            .map_err(|never| match never {})
            .boxed();
        let mut response = Response::new(body);
        *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
        return Ok(response)
    };

    let req = Request::from_parts(parts, bytes);
    let db = match Database::new(&db_path) {
        Ok(db) => db,
        Err(e) => {
            tracing::error!("unable to connect to database: {e:?}");
            let mut res = MainsailResponse::new_with_body("INTERNAL SERVER ERROR");
            res.set_status(StatusCode::INTERNAL_SERVER_ERROR);
            return Ok(res.into_hyper());
        }
    };

    let span = tracing::info_span!("router",
        db = db.get_path(),
        method = req.method().as_str(),
        path = req.uri().path(),
        status_code = tracing::field::Empty
        );

    let _guard = span.enter();
    let result = router::router(req, db).instrument(span.clone()).await;
    let result = result.unwrap_or_else(|e| {
        let mut res = MainsailResponse::new_with_body(e.body());
        res.set_status(e.status());
        res
    });
    span.record("status_code", result.status().as_str());
    Ok(result.into_hyper())
}
