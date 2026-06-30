use bytes::Bytes;
use bytes::BytesMut;
use chrono::{DateTime, Utc};
use futures_core::Stream;
use futures_util::{StreamExt, TryStreamExt};
use reqwest::Response;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::pin::Pin;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[repr(C)]
pub struct EntityList {
    pub entities: Vec<Entity>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[repr(C)]
pub struct Entity {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub dir: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub dir_id: Option<Uuid>,
    pub size: u64,
    pub is_dir: bool,
    pub created: DateTime<Utc>,
    pub last_modified: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[repr(C)]
pub struct AppDto {
    pub id: Uuid,
    pub name: String,
    pub quota: i64,
    pub created: DateTime<Utc>,
    pub last_modified: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[repr(C)]
pub struct BucketDto {
    pub app_id: Uuid,
    pub id: Uuid,
    pub name: String,
    pub encrypted: bool,
    pub atomic_upload: bool,
    pub quota: i64,
    pub file_count: i64,
    pub space_taken: i64,
    pub created: DateTime<Utc>,
    pub last_modified: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[repr(C)]
pub struct UploadSessionStartResponse {
    /// To be used in the path
    pub code: String,
    /// Seconds till the unfinished chunk is dropped when the upload is not reinitialized
    pub validity: u32,
    /// The amount already uploaded to meowith.
    /// The client should resume uploading from there.
    pub uploaded: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[repr(C)]
pub struct UploadSessionResumeResponse {
    /// The number of bytes already uploaded to the meowith store.
    pub uploaded_size: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[repr(C)]
pub struct AppTokenDTO {
    pub created: DateTime<Utc>,
    pub last_modified: DateTime<Utc>,
    pub issuer_id: Uuid,
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[repr(C)]
pub struct TokenListResponse {
    pub tokens: Vec<AppTokenDTO>,
}

#[derive(Debug)]
pub struct FileResponse {
    pub length: u64,
    pub name: String,
    pub mime: String,
    pub response: DownloadBody,
}

#[derive(Debug)]
pub enum DownloadChunkError {
    Http(reqwest::Error),
    Io(std::io::Error),
}

impl Display for DownloadChunkError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadChunkError::Http(error) => write!(f, "{}", error),
            DownloadChunkError::Io(error) => write!(f, "{}", error),
        }
    }
}

impl Error for DownloadChunkError {}

pub enum DownloadBody {
    Http(Response),
    Stream(Pin<Box<dyn Stream<Item = Result<Bytes, DownloadChunkError>> + Send>>),
}

impl std::fmt::Debug for DownloadBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DownloadBody::Http(_) => f.write_str("DownloadBody::Http"),
            DownloadBody::Stream(_) => f.write_str("DownloadBody::Stream"),
        }
    }
}

impl DownloadBody {
    pub fn from_http(response: Response) -> Self {
        Self::Http(response)
    }

    pub fn from_stream<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes, DownloadChunkError>> + Send + 'static,
    {
        Self::Stream(Box::pin(stream))
    }

    pub fn bytes_stream(
        self,
    ) -> Pin<Box<dyn Stream<Item = Result<Bytes, DownloadChunkError>> + Send>> {
        match self {
            DownloadBody::Http(response) => Box::pin(
                response
                    .bytes_stream()
                    .map(|chunk| chunk.map_err(DownloadChunkError::Http)),
            ),
            DownloadBody::Stream(stream) => stream,
        }
    }

    pub async fn bytes(self) -> Result<Bytes, DownloadChunkError> {
        match self {
            DownloadBody::Http(response) => response.bytes().await.map_err(DownloadChunkError::Http),
            DownloadBody::Stream(stream) => {
                let buffer = stream
                    .try_fold(BytesMut::new(), |mut buffer, chunk| async move {
                        buffer.extend_from_slice(&chunk);
                        Ok(buffer)
                    })
                    .await?;
                Ok(buffer.freeze())
            }
        }
    }
}
