use std::str::FromStr;
use crate::connector::headers::{extract_filename};
use crate::dto::request::{RenameEntityRequest, UploadSessionRequest, UploadSessionResumeRequest};
use crate::dto::response::{BucketDto, Entity, EntityList, FileResponse, UploadSessionResumeResponse, UploadSessionStartResponse};
use crate::error::ConnectorError::{Local, Remote};
use crate::error::{ConnectorError, ConnectorResponse, NodeClientError};
use reqwest::header::{
    HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE,
};
use reqwest::{Body, Client, ClientBuilder};
use uuid::Uuid;
use crate::dto::range::{construct_pagination_query, Range};

#[derive(Clone)]
pub struct MeowithConnector {
    client: Client,
    bucket_id: Uuid,
    app_id: Uuid,
    node_addr: String,
}

const CONTENT_LENGTH_HEADER: &str = "X-File-Content-Length";

impl MeowithConnector {
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

    pub async fn upload_oneshot(
        &self,
        stream: Body,
        path: &str,
        size: u64,
    ) -> ConnectorResponse<()> {
        let response = self
            .client
            .post(format!(
                "{}/api/file/upload/oneshot/{}/{}/{}",
                self.node_addr, self.app_id, self.bucket_id, path
            ))
            .header(CONTENT_LENGTH, size.to_string())
            .body(stream)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(ConnectorError::Remote(
                NodeClientError::from(response).await,
            ));
        }
        Ok(())
    }

    pub async fn delete_file(&self, path: &str) -> ConnectorResponse<()> {
        let response = self
            .client
            .delete(format!(
                "{}/api/file/delete/{}/{}/{}",
                self.node_addr, self.app_id, self.bucket_id, path
            ))
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Remote(
                NodeClientError::from(response).await,
            ));
        }
        Ok(())
    }

    pub async fn rename_file(&self, from: &str, to: &str) -> ConnectorResponse<()> {
        let req = RenameEntityRequest { to: to.to_string() };

        let response = self
            .client
            .post(format!(
                "{}/api/file/upload/rename/{}/{}/{}",
                self.node_addr, self.app_id, self.bucket_id, from
            ))
            .json(&req)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Remote(
                NodeClientError::from(response).await,
            ));
        }
        Ok(())
    }

    pub async fn download_file(&self, path: &str) -> ConnectorResponse<FileResponse> {
        let response = self
            .client
            .get(format!(
                "{}/api/file/download/{}/{}/{}",
                self.node_addr, self.app_id, self.bucket_id, path
            ))
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Remote(NodeClientError::from(response).await));
        }

        Ok(FileResponse {
            length: response
                .headers()
                .get(CONTENT_LENGTH_HEADER)
                .ok_or(Local(Box::new(NodeClientError::BadRequest)))?
                .to_str()?
                .to_string()
                .parse::<u64>()?,
            name: extract_filename(
                response
                    .headers()
                    .get(CONTENT_DISPOSITION)
                    .ok_or(Local(Box::new(NodeClientError::BadRequest)))?
                    .to_str()?,
            )
            .ok_or(Local(Box::new(NodeClientError::BadRequest)))?,
            mime: response
                .headers()
                .get(CONTENT_TYPE)
                .ok_or(Local(Box::new(NodeClientError::BadRequest)))?
                .to_str()?
                .to_string(),
            response,
        })
    }

    pub async fn create_directory(&self, path: String) -> ConnectorResponse<()> {
        let response = self
            .client
            .post(format!(
                "{}/api/directory/create/{}/{}/{}",
                self.node_addr, self.app_id, self.bucket_id, path
            ))
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Remote(
                NodeClientError::from(response).await,
            ));
        }
        Ok(())
    }

    pub async fn rename_directory(&self, from: &str, to: &str) -> ConnectorResponse<()> {
        let req = RenameEntityRequest { to: to.to_string() };

        let response = self
            .client
            .post(format!(
                "{}/api/directory/rename/{}/{}/{}",
                self.node_addr, self.app_id, self.bucket_id, from
            ))
            .json(&req)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Remote(
                NodeClientError::from(response).await,
            ));
        }
        Ok(())
    }

    pub async fn delete_directory(&self, path: &str) -> ConnectorResponse<()> {
        let response = self
            .client
            .delete(format!(
                "{}/api/directory/delete/{}/{}/{}",
                self.node_addr, self.app_id, self.bucket_id, path
            ))
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Remote(
                NodeClientError::from(response).await,
            ));
        }
        Ok(())
    }

    pub async fn list_bucket_files(&self, range: Option<Range>) -> ConnectorResponse<EntityList> {
        let response = self
            .client
            .get(format!(
                "{}/api/bucket/list/files/{}/{}{}",
                self.node_addr, self.app_id, self.bucket_id, construct_pagination_query(range)
            ))
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Remote(
                NodeClientError::from(response).await,
            ));
        }

        Ok(response.json::<EntityList>().await.map_err(|e| ConnectorError::from(e))?)
    }

    pub async fn list_bucket_directories(&self, range: Option<Range>) -> ConnectorResponse<EntityList> {
        let response = self
            .client
            .get(format!(
                "{}/api/bucket/list/directories/{}/{}{}",
                self.node_addr, self.app_id, self.bucket_id, construct_pagination_query(range)
            ))
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Remote(
                NodeClientError::from(response).await,
            ));
        }

        Ok(response.json::<EntityList>().await.map_err(|e| ConnectorError::from(e))?)
    }

    pub async fn list_directory(&self, path: String, range: Option<Range>) -> ConnectorResponse<EntityList> {
        let response = self
            .client
            .get(format!(
                "{}/api/directory/list/{}/{}/{}{}",
                self.node_addr, self.app_id, self.bucket_id, path, construct_pagination_query(range)
            ))
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Remote(
                NodeClientError::from(response).await,
            ));
        }

        Ok(response.json::<EntityList>().await.map_err(|e| ConnectorError::from(e))?)
    }

    pub async fn stat_resource(&self, path: String) -> ConnectorResponse<Entity> {
        let response = self
            .client
            .get(format!(
                "{}/api/bucket/stat/{}/{}/{}",
                self.node_addr, self.app_id, self.bucket_id, path
            ))
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Remote(
                NodeClientError::from(response).await,
            ));
        }

        Ok(response.json::<Entity>().await.map_err(|e| ConnectorError::from(e))?)
    }

    pub async fn fetch_bucket_info(&self) -> ConnectorResponse<BucketDto> {
        let response = self
            .client
            .get(format!(
                "{}/api/bucket/info/{}/{}",
                self.node_addr, self.app_id, self.bucket_id
            ))
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Remote(
                NodeClientError::from(response).await,
            ));
        }

        Ok(response.json::<BucketDto>().await.map_err(|e| ConnectorError::from(e))?)
    }

    pub async fn start_upload_session(&self, path: &str, size: u64) -> ConnectorResponse<UploadSessionStartResponse> {
        let req = UploadSessionRequest {
            size
        };
        let response = self
            .client
            .delete(format!(
                "{}/api/file/upload/oneshot/{}/{}/{}",
                self.node_addr, self.app_id, self.bucket_id, path
            ))
            .json(&req)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Remote(
                NodeClientError::from(response).await,
            ));
        }

        Ok(response.json::<UploadSessionStartResponse>().await.map_err(|e| ConnectorError::from(e))?)
    }

    pub async fn resume_upload_session(&self, session: UploadSessionStartResponse) -> ConnectorResponse<UploadSessionResumeResponse> {
        let req = UploadSessionResumeRequest {
            session_id: Uuid::from_str(session.code.as_str())?
        };
        let response = self
            .client
            .delete(format!(
                "{}/api/file/upload/resume/{}/{}",
                self.node_addr, self.app_id, self.bucket_id
            ))
            .json(&req)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Remote(
                NodeClientError::from(response).await,
            ));
        }

        Ok(response.json::<UploadSessionResumeResponse>().await.map_err(|e| ConnectorError::from(e))?)
    }
    pub async fn put_file(&self, session: UploadSessionStartResponse, stream: Body) -> ConnectorResponse<()> {
        let req = UploadSessionResumeRequest {
            session_id: Uuid::from_str(session.code.as_str())?
        };
        let response = self
            .client
            .delete(format!(
                "{}/api/file/upload/put/{}/{}/{}",
                self.node_addr, self.app_id, self.bucket_id, session.code
            ))
            .json(&req)
            .body(stream)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(Remote(
                NodeClientError::from(response).await,
            ));
        }

        Ok(())
    }
}
