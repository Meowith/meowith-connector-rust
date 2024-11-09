use reqwest::header::ToStrError;
use reqwest::{Error, Response};
use serde::Deserialize;
use std::num::ParseIntError;

pub type ConnectorResponse<T> = Result<T, ConnectorError>;

#[derive(Deserialize)]
pub struct ErrorResponse {
    pub code: NodeClientError,
}

#[derive(Clone, Debug, Deserialize)]
pub enum NodeClientError {
    InternalError,
    BadRequest,
    NotFound,
    EntityExists,
    NoSuchSession,
    BadAuth,
    InsufficientStorage,
    NotEmpty,
    RangeUnsatisfiable,
}

impl NodeClientError {
    pub async fn from(value: Response) -> Self {
        match value.json::<ErrorResponse>().await {
            Ok(resp) => resp.code,
            Err(_) => NodeClientError::InternalError,
        }
    }
}

#[derive(Debug)]
#[allow(unused)]
pub enum ConnectorError {
    Remote(NodeClientError),
    Local(Error),
}

impl From<Error> for ConnectorError {
    fn from(value: Error) -> Self {
        ConnectorError::Local(value)
    }
}

impl From<ParseIntError> for ConnectorError {
    fn from(_: ParseIntError) -> Self {
        ConnectorError::Remote(NodeClientError::BadRequest)
    }
}

impl From<ToStrError> for ConnectorError {
    fn from(_: ToStrError) -> Self {
        ConnectorError::Remote(NodeClientError::BadRequest)
    }
}
