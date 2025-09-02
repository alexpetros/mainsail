use crate::error::MainsailError;

pub mod notes;
pub mod actors;
pub mod notifications;
pub mod feeds;

pub type InternalResult<T> = Result<T, MainsailError>;
