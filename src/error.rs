use std::error::Error;
use std::fmt::Display;
use http::StatusCode;
use openssl::error::ErrorStack;

#[derive(Debug)]
pub enum MainsailError {
    ActivityPubError(String),
    BodyTooLarge,
    BodyNotUTF8,
    NotFound,
    Unauthorized(String),
    BadRequest(String),
    BadGateway(String),
    InternalSerializationError,
    FailedToContactActor(reqwest::Error),
    InvalidActorBody(reqwest::Error),
    InvalidActivityPubJson(String),
    InvalidActorUri(String),
    InternalInvalidHeader,
    InternalInvalidRequest(reqwest::Error),
    InternalDeserializationError(String),
    InternalPkeyError,
    InternalServerError(String),
    HyperError(hyper::Error),
    OpenSSLError(ErrorStack),
    SQLError(rusqlite::Error),
    GenericError(Box<dyn Error + Send + Sync>),
}

impl Error for MainsailError {}

impl MainsailError {
    pub fn status(&self) -> StatusCode {
        match self {
            // 400s - broadly speaking, things that are the requester's fault
            MainsailError::BodyTooLarge => StatusCode::BAD_REQUEST,
            MainsailError::BodyNotUTF8 => StatusCode::BAD_REQUEST,
            MainsailError::BadRequest(_) => StatusCode::BAD_REQUEST,
            MainsailError::NotFound => StatusCode::NOT_FOUND,
            MainsailError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            MainsailError::BadGateway(_) => StatusCode::BAD_REQUEST,
            MainsailError::FailedToContactActor(_) => StatusCode::BAD_GATEWAY,
            MainsailError::InvalidActorBody(_) => StatusCode::BAD_GATEWAY,
            MainsailError::InvalidActivityPubJson(_) => StatusCode::BAD_GATEWAY,
            MainsailError::InvalidActorUri(_) => StatusCode::BAD_GATEWAY,

            // 500s - broadly speaking, things that are not the requester's fault
            MainsailError::ActivityPubError(_) => StatusCode::BAD_GATEWAY,
            MainsailError::InternalInvalidHeader => StatusCode::INTERNAL_SERVER_ERROR,
            MainsailError::InternalSerializationError => StatusCode::INTERNAL_SERVER_ERROR,
            MainsailError::InternalInvalidRequest(_) => StatusCode::INTERNAL_SERVER_ERROR,
            MainsailError::InternalDeserializationError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            MainsailError::InternalPkeyError => StatusCode::INTERNAL_SERVER_ERROR,
            MainsailError::InternalServerError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            MainsailError::HyperError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            MainsailError::OpenSSLError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            MainsailError::SQLError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            MainsailError::GenericError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    // What to display to the user if this request is bubbled up
    pub fn body(&self) -> String {
        if self.status().as_u16() < 500 {
            self.to_string()
        } else {
            "Sorry, something went wrong".to_string()
        }
    }

    pub fn new_from_str(s: &str) -> MainsailError {
        MainsailError::InternalServerError(s.into())
    }
}

// What to display in the internal server logs
impl Display for MainsailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MainsailError::BodyTooLarge => write!(f, "Body was too large"),
            MainsailError::BodyNotUTF8 => write!(f, "Body should be UTF8"),
            MainsailError::NotFound => write!(f, "NOT FOUND"),
            MainsailError::Unauthorized(e) => write!(f, "{e}"),
            MainsailError::BadRequest(e) => write!(f, "{e}"),
            MainsailError::BadGateway(e) => write!(f, "{e}"),
            MainsailError::InvalidActorBody(e) => write!(f, "{e}"),
            MainsailError::InvalidActivityPubJson(e) => write!(f, "{e}"),

            MainsailError::InternalInvalidHeader => write!(f, "Attempted to create invalid header"),
            MainsailError::ActivityPubError(s) => write!(f, "ActivityPub service returned error message {s}"),
            MainsailError::InternalSerializationError => write!(f, "Internal serialization error"),
            MainsailError::FailedToContactActor(e) => write!(f, "Failed to contact actor: {e}"),
            MainsailError::InvalidActorUri(e) => write!(f, "Attempted to deserialize invalid actor: {e}"),
            MainsailError::InternalInvalidRequest(e) => write!(f, "Attempted to build invalid request: {e}"),
            MainsailError::InternalDeserializationError(e) => write!(f, "{e}"),
            MainsailError::InternalPkeyError => write!(f, "Internal Private Key Error"),
            MainsailError::InternalServerError(e) => write!(f, "{e}"),
            MainsailError::HyperError(e) => write!(f, "{e}"),
            MainsailError::OpenSSLError(e) => write!(f, "{e}"),
            MainsailError::SQLError(e) => write!(f, "{e}"),
            MainsailError::GenericError(e) => write!(f, "{e}"),
        }
    }
}

impl From<rusqlite::Error> for MainsailError {
    fn from(e: rusqlite::Error) -> Self {
        MainsailError::SQLError(e)
    }
}

impl From<hyper::Error> for MainsailError {
    fn from(e: hyper::Error) -> Self {
        MainsailError::HyperError(e)
    }
}

impl From<ErrorStack> for MainsailError {
    fn from(e: ErrorStack) -> Self {
        MainsailError::OpenSSLError(e)
    }
}

pub fn failed_deserialization(object: String) -> MainsailError {
    MainsailError::InternalDeserializationError(object)
}

pub fn bad_request(message: &str) -> MainsailError {
    MainsailError::BadRequest(message.into())
}

pub fn unauthorized(message: String) -> MainsailError {
    MainsailError::Unauthorized(message)
}

pub fn not_found() -> MainsailError {
    MainsailError::NotFound
}


pub fn bad_gateway(message: &str) -> MainsailError {
    MainsailError::BadGateway(message.into())
}
