use std::error::Error;
use std::fmt::{Display, Formatter};
use reqwest::header::ToStrError;
use reqwest::{Response};
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
impl Error for NodeClientError {
}

impl Display for NodeClientError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(format!("{:?}", self).as_str()).expect("Something went wrong");
        Ok(())
    }
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
    Local(Box<dyn Error>),
}

impl From<Box<dyn Error>> for ConnectorError {
    fn from(value: Box<dyn Error>) -> Self {
        ConnectorError::Local(value)
    }
}

impl From<reqwest::Error> for ConnectorError {
    fn from(value: reqwest::Error) -> Self {
        ConnectorError::Local(Box::new(value))
    }
}

impl From<uuid::Error> for ConnectorError {
    fn from(value: uuid::Error) -> Self {
        ConnectorError::Local(Box::new(value))
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
