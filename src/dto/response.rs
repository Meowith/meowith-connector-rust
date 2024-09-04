use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
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
