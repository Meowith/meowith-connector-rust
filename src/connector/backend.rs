use crate::connector::headers::extract_filename;
use crate::dto::range::{construct_pagination_query, DownloadRange, Range};
use crate::dto::request::{DeleteDirectoryRequest, RenameEntityRequest, UploadSessionRequest, UploadSessionResumeRequest};
use crate::dto::response::{BucketDto, DownloadBody, DownloadChunkError, Entity, EntityList, FileResponse, UploadSessionResumeResponse, UploadSessionStartResponse};
use crate::error::ConnectorError::{Local, Remote};
use crate::error::{ConnectorError, ConnectorResponse, NodeClientError};
use async_trait::async_trait;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use log::{info, trace};
use reqwest::header::{
    HeaderMap, HeaderValue, AUTHORIZATION, CONNECTION, CONTENT_DISPOSITION, CONTENT_LENGTH,
    CONTENT_TYPE, RANGE,
};
use reqwest::{Body, Client, ClientBuilder};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};
use tempfile::TempDir;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
use tokio_util::io::ReaderStream;
use uuid::Uuid;

#[async_trait]
pub trait ConnectorBackend: Send + Sync {
    async fn upload_oneshot(
        &self,
        stream: Body,
        path: &str,
        size: u64,
        upload_id: Option<&str>,
    ) -> ConnectorResponse<()>;
    async fn delete_file(&self, path: &str) -> ConnectorResponse<()>;
    async fn rename_file(&self, from: &str, to: &str) -> ConnectorResponse<()>;
    async fn download_file_range(&self, path: &str, range: DownloadRange) -> ConnectorResponse<FileResponse>;
    async fn create_directory(&self, path: &str) -> ConnectorResponse<()>;
    async fn rename_directory(&self, from: &str, to: &str) -> ConnectorResponse<()>;
    async fn delete_directory(&self, path: &str, recursive: bool) -> ConnectorResponse<()>;
    async fn list_bucket_files(&self, range: Option<Range>) -> ConnectorResponse<EntityList>;
    async fn list_bucket_directories(&self, range: Option<Range>) -> ConnectorResponse<EntityList>;
    async fn list_directory(&self, path: &str, range: Option<Range>) -> ConnectorResponse<EntityList>;
    async fn stat_resource(&self, path: &str) -> ConnectorResponse<Entity>;
    async fn fetch_bucket_info(&self) -> ConnectorResponse<BucketDto>;
    async fn start_upload_session(&self, path: &str, size: u64) -> ConnectorResponse<UploadSessionStartResponse>;
    async fn resume_upload_session(&self, session: UploadSessionStartResponse) -> ConnectorResponse<UploadSessionResumeResponse>;
    async fn put_file(&self, session: UploadSessionStartResponse, stream: Body) -> ConnectorResponse<()>;
}

pub struct HttpBackend {
    client: Client,
    bucket_id: Uuid,
    app_id: Uuid,
    node_addr: String,
}

impl HttpBackend {
    pub fn new(token: &str, bucket_id: Uuid, app_id: Uuid, node_addr: String) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(format!("Bearer {}", token).as_str()).unwrap(),
        );

        Self {
            client: ClientBuilder::new()
                .default_headers(headers)
                .build()
                .unwrap(),
            bucket_id,
            app_id,
            node_addr,
        }
    }

    async fn remote_error(response: reqwest::Response) -> ConnectorError {
        ConnectorError::Remote(NodeClientError::from(response).await)
    }

    fn error_chain(error: &(dyn std::error::Error + 'static)) -> String {
        let mut messages = vec![error.to_string()];
        let mut source = error.source();
        while let Some(error) = source {
            messages.push(error.to_string());
            source = error.source();
        }
        messages.join(" -> ")
    }
}

#[async_trait]
impl ConnectorBackend for HttpBackend {
    async fn upload_oneshot(
        &self,
        stream: Body,
        path: &str,
        size: u64,
        upload_id: Option<&str>,
    ) -> ConnectorResponse<()> {
        let started = Instant::now();
        let upload_id = upload_id.unwrap_or("untracked");
        let body_kind = if stream.as_bytes().is_some() {
            "buffered"
        } else {
            "streaming"
        };
        trace!(target: "kloud::upload",
            "Connector oneshot upload started: upload_id={} path={} declared_bytes={} node={} body_kind={}",
            upload_id, path, size, self.node_addr, body_kind
        );
        let mut request = self
            .client
            .post(format!(
                "{}/api/file/upload/oneshot/{}/{}/{}",
                self.node_addr,
                self.app_id,
                self.bucket_id,
                urlencoding::encode(path)
            ))
            .header(CONTENT_LENGTH, size.to_string());
        if upload_id != "untracked" {
            request = request.header("X-Kloud-Upload-Id", upload_id);
        }
        let response = match request.body(stream).send().await {
            Ok(response) => response,
            Err(err) => {
                trace!(target: "kloud::upload",
                    "Connector oneshot upload transport failed: upload_id={} path={} declared_bytes={} body_kind={} elapsed_ms={} is_timeout={} is_connect={} is_request={} is_body={} is_decode={} url={:?} error_chain={}",
                    upload_id,
                    path,
                    size,
                    body_kind,
                    started.elapsed().as_millis(),
                    err.is_timeout(),
                    err.is_connect(),
                    err.is_request(),
                    err.is_body(),
                    err.is_decode(),
                    err.url(),
                    Self::error_chain(&err)
                );
                return Err(err.into());
            }
        };
        let status = response.status();
        let version = response.version();
        let response_content_length = response.content_length();
        let connection = response
            .headers()
            .get(CONNECTION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("unspecified");
        trace!(target: "kloud::upload",
            "Connector oneshot upload response received: upload_id={} path={} declared_bytes={} status={} version={:?} response_content_length={:?} connection={} elapsed_ms={}",
            upload_id,
            path,
            size,
            status,
            version,
            response_content_length,
            connection,
            started.elapsed().as_millis()
        );
        if !response.status().is_success() {
            let error = Self::remote_error(response).await;
            trace!(target: "kloud::upload",
                "Connector oneshot upload rejected: upload_id={} path={} declared_bytes={} body_kind={} status={} elapsed_ms={} error={:?}",
                upload_id,
                path,
                size,
                body_kind,
                status,
                started.elapsed().as_millis(),
                error
            );
            return Err(error);
        }
        trace!(target: "kloud::upload",
            "Connector oneshot upload completed: upload_id={} path={} declared_bytes={} body_kind={} elapsed_ms={}",
            upload_id,
            path,
            size,
            body_kind,
            started.elapsed().as_millis()
        );
        Ok(())
    }

    async fn delete_file(&self, path: &str) -> ConnectorResponse<()> {
        let response = self
            .client
            .delete(format!(
                "{}/api/file/delete/{}/{}/{}",
                self.node_addr,
                self.app_id,
                self.bucket_id,
                urlencoding::encode(path)
            ))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(Self::remote_error(response).await);
        }
        Ok(())
    }

    async fn rename_file(&self, from: &str, to: &str) -> ConnectorResponse<()> {
        let req = RenameEntityRequest { to: to.to_string() };
        let response = self
            .client
            .post(format!(
                "{}/api/file/rename/{}/{}/{}",
                self.node_addr,
                self.app_id,
                self.bucket_id,
                urlencoding::encode(from)
            ))
            .json(&req)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(Self::remote_error(response).await);
        }
        Ok(())
    }

    async fn download_file_range(&self, path: &str, range: DownloadRange) -> ConnectorResponse<FileResponse> {
        let url = format!(
            "{}/api/file/download/{}/{}/{}",
            self.node_addr,
            self.app_id,
            self.bucket_id,
            urlencoding::encode(path)
        );
        info!(
            "Connector download request: path={} url={}",
            path, url
        );
        let mut request = self.client.get(url);
        if !range.is_full() {
            request = request.header(RANGE, range.header_value());
        }
        
        let response = match request.send().await {
            Ok(response) => response,
            Err(err) => {
                info!(
                    "Connector download transport failed: path={} range={:?} error={:?}",
                    path, range, err
                );
                return Err(err.into());
            }
        };

        if !response.status().is_success() {
            return Err(Self::remote_error(response).await);
        }

        Ok(FileResponse {
            length: response
                .headers()
                .get("X-File-Content-Length")
                .ok_or(Local(Box::new(NodeClientError::BadRequest)))?
                .to_str()?
                .parse::<u64>()?,
            name: extract_filename(
                response
                    .headers()
                    .get(CONTENT_DISPOSITION)
                    .ok_or(Local(Box::new(NodeClientError::BadRequest)))?.as_bytes()
            )
            .ok_or(Local(Box::new(NodeClientError::BadRequest)))?,
            mime: response
                .headers()
                .get(CONTENT_TYPE)
                .ok_or(Local(Box::new(NodeClientError::BadRequest)))?
                .to_str()?
                .to_string(),
            response: DownloadBody::from_http(response),
        })
    }

    async fn create_directory(&self, path: &str) -> ConnectorResponse<()> {
        let response = self
            .client
            .post(format!(
                "{}/api/directory/create/{}/{}/{}",
                self.node_addr,
                self.app_id,
                self.bucket_id,
                urlencoding::encode(path)
            ))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(Self::remote_error(response).await);
        }
        Ok(())
    }

    async fn rename_directory(&self, from: &str, to: &str) -> ConnectorResponse<()> {
        let req = RenameEntityRequest { to: to.to_string() };
        let response = self
            .client
            .post(format!(
                "{}/api/directory/rename/{}/{}/{}",
                self.node_addr,
                self.app_id,
                self.bucket_id,
                urlencoding::encode(from)
            ))
            .json(&req)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(Self::remote_error(response).await);
        }
        Ok(())
    }

    async fn delete_directory(&self, path: &str, recursive: bool) -> ConnectorResponse<()> {
        let req = DeleteDirectoryRequest { recursive };
        let response = self
            .client
            .delete(format!(
                "{}/api/directory/delete/{}/{}/{}",
                self.node_addr,
                self.app_id,
                self.bucket_id,
                urlencoding::encode(path)
            ))
            .json(&req)
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(Self::remote_error(response).await);
        }
        Ok(())
    }

    async fn list_bucket_files(&self, range: Option<Range>) -> ConnectorResponse<EntityList> {
        let response = self
            .client
            .get(format!(
                "{}/api/bucket/list/files/{}/{}{}",
                self.node_addr,
                self.app_id,
                self.bucket_id,
                construct_pagination_query(range)
            ))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(Self::remote_error(response).await);
        }
        response.json::<EntityList>().await.map_err(ConnectorError::from)
    }

    async fn list_bucket_directories(&self, range: Option<Range>) -> ConnectorResponse<EntityList> {
        let response = self
            .client
            .get(format!(
                "{}/api/bucket/list/directories/{}/{}{}",
                self.node_addr,
                self.app_id,
                self.bucket_id,
                construct_pagination_query(range)
            ))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(Self::remote_error(response).await);
        }
        response.json::<EntityList>().await.map_err(ConnectorError::from)
    }

    async fn list_directory(&self, path: &str, range: Option<Range>) -> ConnectorResponse<EntityList> {
        let response = self
            .client
            .get(format!(
                "{}/api/directory/list/{}/{}/{}{}",
                self.node_addr,
                self.app_id,
                self.bucket_id,
                urlencoding::encode(path),
                construct_pagination_query(range)
            ))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(Self::remote_error(response).await);
        }
        response.json::<EntityList>().await.map_err(ConnectorError::from)
    }

    async fn stat_resource(&self, path: &str) -> ConnectorResponse<Entity> {
        let response = self
            .client
            .get(format!(
                "{}/api/bucket/stat/{}/{}/{}",
                self.node_addr,
                self.app_id,
                self.bucket_id,
                urlencoding::encode(path)
            ))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(Self::remote_error(response).await);
        }
        response.json::<Entity>().await.map_err(ConnectorError::from)
    }

    async fn fetch_bucket_info(&self) -> ConnectorResponse<BucketDto> {
        let response = self
            .client
            .get(format!("{}/api/bucket/info/{}/{}", self.node_addr, self.app_id, self.bucket_id))
            .send()
            .await?;
        if !response.status().is_success() {
            return Err(Self::remote_error(response).await);
        }
        response.json::<BucketDto>().await.map_err(ConnectorError::from)
    }

    async fn start_upload_session(&self, path: &str, size: u64) -> ConnectorResponse<UploadSessionStartResponse> {
        let req = UploadSessionRequest { size };
        let started = Instant::now();
        trace!(target: "kloud::upload",
            "Connector durable session start: path={} declared_bytes={} node={}",
            path, size, self.node_addr
        );
        let response = match self
            .client
            .post(format!(
                "{}/api/file/upload/durable/{}/{}/{}",
                self.node_addr,
                self.app_id,
                self.bucket_id,
                urlencoding::encode(path)
            ))
            .json(&req)
            .send()
            .await
        {
            Ok(response) => response,
            Err(err) => {
                trace!(target: "kloud::upload",
                    "Connector durable session start transport failed: path={} declared_bytes={} elapsed_ms={} error={:?}",
                    path,
                    size,
                    started.elapsed().as_millis(),
                    err
                );
                return Err(err.into());
            }
        };
        if !response.status().is_success() {
            let status = response.status();
            let error = Self::remote_error(response).await;
            trace!(target: "kloud::upload",
                "Connector durable session start rejected: path={} declared_bytes={} status={} elapsed_ms={} error={:?}",
                path,
                size,
                status,
                started.elapsed().as_millis(),
                error
            );
            return Err(error);
        }
        let session = response
            .json::<UploadSessionStartResponse>()
            .await
            .map_err(ConnectorError::from)?;
        trace!(target: "kloud::upload",
            "Connector durable session started: path={} session_id={} declared_bytes={} elapsed_ms={}",
            path,
            session.code,
            size,
            started.elapsed().as_millis()
        );
        Ok(session)
    }

    async fn resume_upload_session(&self, session: UploadSessionStartResponse) -> ConnectorResponse<UploadSessionResumeResponse> {
        let req = UploadSessionResumeRequest {
            session_id: Uuid::from_str(session.code.as_str())?,
        };
        let started = Instant::now();
        trace!(target: "kloud::upload",
            "Connector durable session resume: session_id={} node={}",
            session.code, self.node_addr
        );
        let response = match self
            .client
            .post(format!("{}/api/file/upload/resume/{}/{}", self.node_addr, self.app_id, self.bucket_id))
            .json(&req)
            .send()
            .await
        {
            Ok(response) => response,
            Err(err) => {
                trace!(target: "kloud::upload",
                    "Connector durable session resume transport failed: session_id={} elapsed_ms={} error={:?}",
                    session.code,
                    started.elapsed().as_millis(),
                    err
                );
                return Err(err.into());
            }
        };
        if !response.status().is_success() {
            let status = response.status();
            let error = Self::remote_error(response).await;
            trace!(target: "kloud::upload",
                "Connector durable session resume rejected: session_id={} status={} elapsed_ms={} error={:?}",
                session.code,
                status,
                started.elapsed().as_millis(),
                error
            );
            return Err(error);
        }
        let resumed = response
            .json::<UploadSessionResumeResponse>()
            .await
            .map_err(ConnectorError::from)?;
        trace!(target: "kloud::upload",
            "Connector durable session resumed: session_id={} uploaded_bytes={} elapsed_ms={}",
            session.code,
            resumed.uploaded_size,
            started.elapsed().as_millis()
        );
        Ok(resumed)
    }

    async fn put_file(&self, session: UploadSessionStartResponse, stream: Body) -> ConnectorResponse<()> {
        let started = Instant::now();
        trace!(target: "kloud::upload",
            "Connector durable chunk started: session_id={} offset={} node={}",
            session.code, session.uploaded, self.node_addr
        );
        let response = match self
            .client
            .put(format!(
                "{}/api/file/upload/put/{}/{}/{}",
                self.node_addr, self.app_id, self.bucket_id, session.code
            ))
            .body(stream)
            .send()
            .await
        {
            Ok(response) => response,
            Err(err) => {
                trace!(target: "kloud::upload",
                    "Connector durable chunk transport failed: session_id={} offset={} elapsed_ms={} error={:?}",
                    session.code,
                    session.uploaded,
                    started.elapsed().as_millis(),
                    err
                );
                return Err(err.into());
            }
        };
        if !response.status().is_success() {
            let status = response.status();
            let error = Self::remote_error(response).await;
            trace!(target: "kloud::upload",
                "Connector durable chunk rejected: session_id={} offset={} status={} elapsed_ms={} error={:?}",
                session.code,
                session.uploaded,
                status,
                started.elapsed().as_millis(),
                error
            );
            return Err(error);
        }
        trace!(target: "kloud::upload",
            "Connector durable chunk completed: session_id={} offset={} elapsed_ms={}",
            session.code,
            session.uploaded,
            started.elapsed().as_millis()
        );
        Ok(())
    }
}

enum DummyRoot {
    Temp(TempDir),
    Persistent(PathBuf),
}

impl DummyRoot {
    fn path(&self) -> &Path {
        match self {
            DummyRoot::Temp(tempdir) => tempdir.path(),
            DummyRoot::Persistent(path) => path.as_path(),
        }
    }
}

struct DummySession {
    path: PathBuf,
    session_path: PathBuf,
}

pub struct DummyBackend {
    root: DummyRoot,
    bucket_id: Uuid,
    app_id: Uuid,
    sessions: Arc<Mutex<HashMap<Uuid, DummySession>>>,
}

impl DummyBackend {
    pub fn new_tempdir(bucket_id: Uuid, app_id: Uuid) -> Self {
        Self {
            root: DummyRoot::Temp(TempDir::new().expect("failed to create temporary dummy connector directory")),
            bucket_id,
            app_id,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn new_persistent(root: impl Into<PathBuf>, bucket_id: Uuid, app_id: Uuid) -> Self {
        let root = root.into();
        fs::create_dir_all(&root).expect("failed to create persistent dummy connector directory");
        Self {
            root: DummyRoot::Persistent(root),
            bucket_id,
            app_id,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn root_path(&self) -> &Path {
        self.root.path()
    }

    fn storage_path(&self, path: &str) -> ConnectorResponse<PathBuf> {
        let mut resolved = PathBuf::from(self.root_path());
        for segment in path.split('/') {
            if segment.is_empty() || segment == "." {
                continue;
            }
            if segment == ".." {
                return Err(Remote(NodeClientError::BadRequest));
            }
            resolved.push(segment);
        }
        Ok(resolved)
    }

    fn io_error(error: std::io::Error) -> ConnectorError {
        match error.kind() {
            std::io::ErrorKind::NotFound => Remote(NodeClientError::NotFound),
            std::io::ErrorKind::AlreadyExists => Remote(NodeClientError::EntityExists),
            std::io::ErrorKind::DirectoryNotEmpty => Remote(NodeClientError::NotEmpty),
            std::io::ErrorKind::InvalidInput => Remote(NodeClientError::BadRequest),
            _ => Local(Box::new(error)),
        }
    }

    fn body_bytes(stream: Body) -> ConnectorResponse<Bytes> {
        stream
            .as_bytes()
            .map(Bytes::copy_from_slice)
            .ok_or_else(|| Local(Box::new(NodeClientError::BadRequest)))
    }

    fn system_time_to_utc(time: SystemTime) -> DateTime<Utc> {
        DateTime::<Utc>::from(time)
    }

    fn entity_from_path(&self, path: &Path) -> ConnectorResponse<Entity> {
        let meta = fs::metadata(path).map_err(Self::io_error)?;
        let name = path
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/".to_string());
        let created = meta.created().or_else(|_| meta.modified()).unwrap_or(SystemTime::now());
        let last_modified = meta.modified().unwrap_or(created);
        Ok(Entity {
            name,
            dir: None,
            dir_id: None,
            size: if meta.is_dir() { 0 } else { meta.len() },
            is_dir: meta.is_dir(),
            created: Self::system_time_to_utc(created),
            last_modified: Self::system_time_to_utc(last_modified),
        })
    }

    fn collect_listing(&self, path: &Path, recursive: bool, include_dirs: bool, entries: &mut Vec<Entity>) -> ConnectorResponse<()> {
        let meta = fs::metadata(path).map_err(Self::io_error)?;
        if meta.is_dir() {
            if include_dirs && path != self.root_path() {
                entries.push(self.entity_from_path(path)?);
            }
            for entry in fs::read_dir(path).map_err(Self::io_error)? {
                let entry = entry.map_err(Self::io_error)?;
                let entry_path = entry.path();
                let entry_meta = entry.metadata().map_err(Self::io_error)?;
                if entry_meta.is_dir() {
                    if recursive {
                        self.collect_listing(&entry_path, recursive, include_dirs, entries)?;
                    } else if include_dirs {
                        entries.push(self.entity_from_path(&entry_path)?);
                    }
                } else if !include_dirs {
                    entries.push(self.entity_from_path(&entry_path)?);
                }
            }
        } else if !include_dirs {
            entries.push(self.entity_from_path(path)?);
        }
        Ok(())
    }

    fn session_path(&self, session_id: Uuid) -> ConnectorResponse<PathBuf> {
        let session_dir = self.root_path().join(".sessions");
        fs::create_dir_all(&session_dir).map_err(Self::io_error)?;
        Ok(session_dir.join(format!("{}.bin", session_id)))
    }

    fn bucket_stats(&self) -> ConnectorResponse<(i64, i64)> {
        let mut entries = Vec::new();
        self.collect_listing(self.root_path(), true, false, &mut entries)?;
        let file_count = entries.len() as i64;
        let space_taken = entries.iter().map(|entry| entry.size as i64).sum();
        Ok((file_count, space_taken))
    }
}

#[async_trait]
impl ConnectorBackend for DummyBackend {
    async fn upload_oneshot(
        &self,
        stream: Body,
        path: &str,
        size: u64,
        _upload_id: Option<&str>,
    ) -> ConnectorResponse<()> {
        let path = self.storage_path(path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(Self::io_error)?;
        }
        let bytes = Self::body_bytes(stream)?;
        let _ = size;
        fs::write(path, bytes).map_err(Self::io_error)?;
        Ok(())
    }

    async fn delete_file(&self, path: &str) -> ConnectorResponse<()> {
        let path = self.storage_path(path)?;
        fs::remove_file(path).map_err(Self::io_error)?;
        Ok(())
    }

    async fn rename_file(&self, from: &str, to: &str) -> ConnectorResponse<()> {
        let from = self.storage_path(from)?;
        let to = self.storage_path(to)?;
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(Self::io_error)?;
        }
        fs::rename(from, to).map_err(Self::io_error)?;
        Ok(())
    }

    async fn download_file_range(&self, path: &str, range: DownloadRange) -> ConnectorResponse<FileResponse> {
        let path = self.storage_path(path)?;
        let meta = fs::metadata(&path).map_err(Self::io_error)?;
        if meta.is_dir() {
            return Err(Remote(NodeClientError::BadRequest));
        }

        let file_len = meta.len();
        let (start, length) = match (range.start, range.end) {
            (Some(start), Some(end)) => {
                if end < start {
                    return Err(Remote(NodeClientError::RangeUnsatisfiable));
                }
                (start, end - start + 1)
            }
            (Some(start), None) => {
                if start > file_len {
                    return Err(Remote(NodeClientError::RangeUnsatisfiable));
                }
                (start, file_len.saturating_sub(start))
            }
            (None, Some(end)) => {
                if end >= file_len {
                    (0, file_len)
                } else {
                    (file_len - end, end)
                }
            }
            (None, None) => (0, file_len),
        };

        let mut file = File::open(&path).await.map_err(Self::io_error)?;
        if start > 0 {
            file.seek(SeekFrom::Start(start)).await.map_err(Self::io_error)?;
        }
        let reader = file.take(length);
        let stream = ReaderStream::new(reader).map(|chunk| chunk.map_err(DownloadChunkError::Io));

        Ok(FileResponse {
            length,
            name: path
                .file_name()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned()),
            mime: mime_guess::from_path(&path).first_or_octet_stream().to_string(),
            response: DownloadBody::from_stream(stream),
        })
    }

    async fn create_directory(&self, path: &str) -> ConnectorResponse<()> {
        let path = self.storage_path(path)?;
        fs::create_dir_all(path).map_err(Self::io_error)?;
        Ok(())
    }

    async fn rename_directory(&self, from: &str, to: &str) -> ConnectorResponse<()> {
        let from = self.storage_path(from)?;
        let to = self.storage_path(to)?;
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(Self::io_error)?;
        }
        fs::rename(from, to).map_err(Self::io_error)?;
        Ok(())
    }

    async fn delete_directory(&self, path: &str, recursive: bool) -> ConnectorResponse<()> {
        let path = self.storage_path(path)?;
        if path == self.root_path() {
            if recursive {
                for entry in fs::read_dir(&path).map_err(Self::io_error)? {
                    let entry = entry.map_err(Self::io_error)?;
                    if entry.metadata().map_err(Self::io_error)?.is_dir() {
                        fs::remove_dir_all(entry.path()).map_err(Self::io_error)?;
                    } else {
                        fs::remove_file(entry.path()).map_err(Self::io_error)?;
                    }
                }
                return Ok(());
            }
            if fs::read_dir(&path).map_err(Self::io_error)?.next().is_some() {
                return Err(Remote(NodeClientError::NotEmpty));
            }
            return Ok(());
        }

        if recursive {
            fs::remove_dir_all(path).map_err(Self::io_error)?;
        } else {
            fs::remove_dir(path).map_err(Self::io_error)?;
        }
        Ok(())
    }

    async fn list_bucket_files(&self, _range: Option<Range>) -> ConnectorResponse<EntityList> {
        let mut entries = Vec::new();
        self.collect_listing(self.root_path(), true, false, &mut entries)?;
        Ok(EntityList { entities: entries })
    }

    async fn list_bucket_directories(&self, _range: Option<Range>) -> ConnectorResponse<EntityList> {
        let mut entries = Vec::new();
        self.collect_listing(self.root_path(), true, true, &mut entries)?;
        entries.retain(|entry| entry.is_dir);
        Ok(EntityList { entities: entries })
    }

    async fn list_directory(&self, path: &str, _range: Option<Range>) -> ConnectorResponse<EntityList> {
        let path = self.storage_path(path)?;
        if !fs::metadata(&path).map_err(Self::io_error)?.is_dir() {
            return Err(Remote(NodeClientError::BadRequest));
        }
        let mut entries = Vec::new();
        self.collect_listing(&path, false, true, &mut entries)?;
        Ok(EntityList { entities: entries })
    }

    async fn stat_resource(&self, path: &str) -> ConnectorResponse<Entity> {
        let path = self.storage_path(path)?;
        self.entity_from_path(&path)
    }

    async fn fetch_bucket_info(&self) -> ConnectorResponse<BucketDto> {
        let (file_count, space_taken) = self.bucket_stats()?;
        let now = Utc::now();
        Ok(BucketDto {
            app_id: self.app_id,
            id: self.bucket_id,
            name: "dummy".to_string(),
            encrypted: false,
            atomic_upload: false,
            quota: -1,
            file_count,
            space_taken,
            created: now,
            last_modified: now,
        })
    }

    async fn start_upload_session(&self, path: &str, _size: u64) -> ConnectorResponse<UploadSessionStartResponse> {
        let session_id = Uuid::new_v4();
        let target_path = self.storage_path(path)?;
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(Self::io_error)?;
        }
        let session_path = self.session_path(session_id)?;
        fs::write(&session_path, &[][..]).map_err(Self::io_error)?;
        self.sessions.lock().expect("dummy connector sessions poisoned").insert(
            session_id,
            DummySession {
                path: target_path,
                session_path,
            },
        );
        Ok(UploadSessionStartResponse {
            code: session_id.to_string(),
            validity: 0,
            uploaded: 0,
        })
    }

    async fn resume_upload_session(&self, session: UploadSessionStartResponse) -> ConnectorResponse<UploadSessionResumeResponse> {
        let session_id = Uuid::from_str(session.code.as_str())?;
        let session_path = {
            let guard = self.sessions.lock().expect("dummy connector sessions poisoned");
            guard.get(&session_id).map(|value| value.session_path.clone()).ok_or(Remote(NodeClientError::NoSuchSession))?
        };
        let uploaded_size = fs::metadata(session_path).map_err(Self::io_error)?.len();
        Ok(UploadSessionResumeResponse { uploaded_size })
    }

    async fn put_file(&self, session: UploadSessionStartResponse, stream: Body) -> ConnectorResponse<()> {
        let session_id = Uuid::from_str(session.code.as_str())?;
        let session = self
            .sessions
            .lock()
            .expect("dummy connector sessions poisoned")
            .remove(&session_id)
            .ok_or(Remote(NodeClientError::NoSuchSession))?;
        let bytes = Self::body_bytes(stream)?;
        fs::write(&session.session_path, &bytes).map_err(Self::io_error)?;
        if let Some(parent) = session.path.parent() {
            fs::create_dir_all(parent).map_err(Self::io_error)?;
        }
        fs::write(session.path, bytes).map_err(Self::io_error)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::mpsc;
    use std::thread;
    use std::time::{Duration, Instant};

    fn read_request(socket: &mut TcpStream) -> String {
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = socket.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
        }

        let header_end = request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|index| index + 4)
            .unwrap_or(request.len());
        let (request_line, content_length) = {
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let request_line = headers.lines().next().unwrap().to_string();
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                })
                .unwrap_or(0);
            (request_line, content_length)
        };
        while request.len() < header_end + content_length {
            let read = socket.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
        }

        request_line
    }

    #[test]
    fn durable_upload_uses_storage_api_methods() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let session_id = Uuid::new_v4();
        let (request_tx, request_rx) = mpsc::channel();

        let server = thread::spawn(move || {
            let responses = [
                format!(
                    "{{\"code\":\"{}\",\"validity\":60,\"uploaded\":0}}",
                    session_id
                ),
                "{\"uploaded_size\":5}".to_string(),
                String::new(),
            ];

            for response_body in responses {
                let (mut socket, _) = listener.accept().unwrap();
                request_tx.send(read_request(&mut socket)).unwrap();
                socket
                    .write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            response_body.len(),
                            response_body
                        )
                        .as_bytes(),
                    )
                    .unwrap();
            }
        });

        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let backend = HttpBackend::new(
                    "test",
                    Uuid::nil(),
                    Uuid::nil(),
                    format!("http://{address}"),
                );
                let session = backend
                    .start_upload_session("test.bin", 10)
                    .await
                    .unwrap();
                backend
                    .resume_upload_session(session.clone())
                    .await
                    .unwrap();
                backend
                    .put_file(session, Body::from("chunk"))
                    .await
                    .unwrap();
            });

        let requests: Vec<String> = request_rx.iter().take(3).collect();
        assert!(requests[0].starts_with("POST /api/file/upload/durable/"));
        assert!(requests[1].starts_with("POST /api/file/upload/resume/"));
        assert!(requests[2].starts_with("PUT /api/file/upload/put/"));
        server.join().unwrap();
    }

    #[test]
    fn dropping_download_stream_closes_upstream_response() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (disconnect_tx, disconnect_rx) = mpsc::channel();

        let server = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = socket.read(&mut buffer).unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
            }

            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\n\
Content-Type: application/octet-stream\r\n\
Content-Disposition: attachment; filename=\"test.bin\"\r\n\
X-File-Content-Length: 1073741824\r\n\
Content-Length: 1073741824\r\n\
\r\n",
                )
                .unwrap();
            socket.set_nonblocking(true).unwrap();

            let chunk = [0_u8; 64 * 1024];
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                match socket.write(&chunk) {
                    Ok(_) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => {
                        disconnect_tx.send(true).unwrap();
                        return;
                    }
                }
            }
            disconnect_tx.send(false).unwrap();
        });

        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let backend = HttpBackend::new(
                    "test",
                    Uuid::nil(),
                    Uuid::nil(),
                    format!("http://{address}"),
                );
                let response = backend
                    .download_file_range("test.bin", DownloadRange::full())
                    .await
                    .unwrap();
                let mut stream = response.response.bytes_stream();
                assert!(!stream.next().await.unwrap().unwrap().is_empty());
                drop(stream);
            });

        assert!(
            disconnect_rx.recv_timeout(Duration::from_secs(6)).unwrap(),
            "upstream socket remained open after the consumer dropped the download stream"
        );
        server.join().unwrap();
    }
}
