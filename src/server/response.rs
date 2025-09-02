use std::convert::Infallible;
use std::ops::{Deref, DerefMut};

use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full};
use hyper::body::Bytes;
use http::header::{HeaderName, HeaderValue};
use http::{Response, StatusCode};

use crate::error::MainsailError;
pub type HyperResponse = Response<BoxBody<Bytes, Infallible>>;
pub type ServerResult = Result<MainsailResponse, MainsailError>;

const _30_DAYS_IN_SECONDS: i64 = 2592000;

#[derive(Debug)]
pub struct MainsailResponse {
    response: Response<BoxBody<Bytes, Infallible>>
}

impl MainsailResponse {
    /*
     * Builder functions for building a custom response.
     * These return the response, *not* a result, so that you can keep editing.
     */
    pub fn new() -> Self {
        let body = Empty::<Bytes>::new()
            .map_err(|never| match never {})
            .boxed();
        let response = Response::new(body);
        MainsailResponse { response }
    }

    pub fn new_with_body<T: Into<Bytes>>(chunk: T) -> Self {
        let body = Full::new(chunk.into())
            .map_err(|never| match never {})
            .boxed();
        let response = Response::new(body);
        MainsailResponse { response }
    }

    pub fn status(&self) -> StatusCode {
        self.response.status()
    }

    pub fn set_status(&mut self, status: StatusCode) {
        let res = &mut self.response;
        *res.status_mut() = status;
    }

    pub fn set_header(&mut self, name: HeaderName, value: HeaderValue) {
        self.headers_mut().append(name, value);
    }

    /// Consume the wrapper and send a hyper-compatible response
    pub fn into_hyper(self) -> HyperResponse {
        self.response
    }

    /// Test function - consume the wrapper and send a hyper-compatible response
    #[cfg(test)]
    pub async fn into_body_string(self) -> String {
        let body_bytes = self.into_hyper().into_body().collect().await.unwrap().to_bytes();
        String::from_utf8(body_bytes.to_vec()).unwrap()
    }
}

impl Deref for MainsailResponse {
    type Target = HyperResponse;

    fn deref(&self) -> &Self::Target {
        &self.response
    }
}

impl DerefMut for MainsailResponse {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.response
    }
}

impl Default for MainsailResponse {
    fn default() -> Self {
        Self::new()
    }
}

// Convenience response functions.
// These all return ServerResults, and are designed to end a request.

pub fn send<T: Into<Bytes>>(chunk: T) -> ServerResult {
    let res = MainsailResponse::new_with_body(chunk);
    Ok(res)
}

pub fn ok() -> ServerResult {
    let res = MainsailResponse::new_with_body("OK");
    Ok(res)
}

pub fn not_found() -> ServerResult {
    let mut res = MainsailResponse::new_with_body("NOT FOUND");
    res.set_status(StatusCode::NOT_FOUND);
    Ok(res)
}

pub fn not_implemented() -> ServerResult {
    let mut res = MainsailResponse::new_with_body("NOT IMPLEMENTED");
    res.set_status(StatusCode::NOT_IMPLEMENTED);
    Ok(res)
}
