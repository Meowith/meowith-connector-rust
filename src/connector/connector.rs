use crate::connector::backend::{ConnectorBackend, DummyBackend, HttpBackend};
use crate::dto::range::{DownloadRange, Range};
use crate::dto::response::{BucketDto, Entity, EntityList, FileResponse, UploadSessionResumeResponse, UploadSessionStartResponse};
use crate::error::ConnectorResponse;
use reqwest::Body;
use std::path::PathBuf;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct MeowithConnector {
    backend: Arc<dyn ConnectorBackend>,
}

impl MeowithConnector {
    pub fn new(token: &str, bucket_id: Uuid, app_id: Uuid, node_addr: String) -> Self {
        Self {
            backend: Arc::new(HttpBackend::new(token, bucket_id, app_id, node_addr)),
        }
    }

    pub fn new_dummy(bucket_id: Uuid, app_id: Uuid) -> Self {
        Self {
            backend: Arc::new(DummyBackend::new_tempdir(bucket_id, app_id)),
        }
    }

    pub fn new_dummy_in_dir(storage_root: impl Into<PathBuf>, bucket_id: Uuid, app_id: Uuid) -> Self {
        Self {
            backend: Arc::new(DummyBackend::new_persistent(storage_root, bucket_id, app_id)),
        }
    }

    pub async fn upload_oneshot(&self, stream: Body, path: &str, size: u64) -> ConnectorResponse<()> {
        self.backend.upload_oneshot(stream, path, size, None).await
    }

    pub async fn upload_oneshot_traced(
        &self,
        stream: Body,
        path: &str,
        size: u64,
        upload_id: &str,
    ) -> ConnectorResponse<()> {
        self.backend
            .upload_oneshot(stream, path, size, Some(upload_id))
            .await
    }

    pub async fn delete_file(&self, path: &str) -> ConnectorResponse<()> {
        self.backend.delete_file(path).await
    }

    pub async fn rename_file(&self, from: &str, to: &str) -> ConnectorResponse<()> {
        self.backend.rename_file(from, to).await
    }

    pub async fn download_file_range(&self, path: &str, range: DownloadRange) -> ConnectorResponse<FileResponse> {
        self.backend.download_file_range(path, range).await
    }

    pub async fn download_file(&self, path: &str) -> ConnectorResponse<FileResponse> {
        self.download_file_range(path, DownloadRange::full()).await
    }

    pub async fn create_directory(&self, path: &str) -> ConnectorResponse<()> {
        self.backend.create_directory(path).await
    }

    pub async fn rename_directory(&self, from: &str, to: &str) -> ConnectorResponse<()> {
        self.backend.rename_directory(from, to).await
    }

    pub async fn delete_directory(&self, path: &str, recursive: bool) -> ConnectorResponse<()> {
        self.backend.delete_directory(path, recursive).await
    }

    pub async fn list_bucket_files(&self, range: Option<Range>) -> ConnectorResponse<EntityList> {
        self.backend.list_bucket_files(range).await
    }

    pub async fn list_bucket_directories(&self, range: Option<Range>) -> ConnectorResponse<EntityList> {
        self.backend.list_bucket_directories(range).await
    }

    pub async fn list_directory(&self, path: &str, range: Option<Range>) -> ConnectorResponse<EntityList> {
        self.backend.list_directory(path, range).await
    }

    pub async fn stat_resource(&self, path: &str) -> ConnectorResponse<Entity> {
        self.backend.stat_resource(path).await
    }

    pub async fn fetch_bucket_info(&self) -> ConnectorResponse<BucketDto> {
        self.backend.fetch_bucket_info().await
    }

    pub async fn start_upload_session(&self, path: &str, size: u64) -> ConnectorResponse<UploadSessionStartResponse> {
        self.backend.start_upload_session(path, size).await
    }

    pub async fn resume_upload_session(&self, session: UploadSessionStartResponse) -> ConnectorResponse<UploadSessionResumeResponse> {
        self.backend.resume_upload_session(session).await
    }

    pub async fn put_file(&self, session: UploadSessionStartResponse, stream: Body) -> ConnectorResponse<()> {
        self.backend.put_file(session, stream).await
    }
}
