use crate::connector::headers::{extract_filename, DESERIALIZE_ERROR};
use crate::dto::request::RenameEntityRequest;
use crate::dto::response::FileResponse;
use crate::error::ConnectorError::Remote;
use crate::error::{ConnectorError, ConnectorResponse, NodeClientError};
use reqwest::header::{
    HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE,
};
use reqwest::{Body, Client, ClientBuilder};
use uuid::Uuid;

#[derive(Clone)]
pub struct MeowithConnector {
    client: Client,
    bucket_id: Uuid,
    app_id: Uuid,
    node_addr: String,
}

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
            return Err(ConnectorError::Remote(
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
            return Err(ConnectorError::Remote(
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
                .get(CONTENT_LENGTH)
                .ok_or(DESERIALIZE_ERROR)?
                .to_str()?
                .to_string()
                .parse::<u64>()?,
            name: extract_filename(
                response
                    .headers()
                    .get(CONTENT_DISPOSITION)
                    .ok_or(DESERIALIZE_ERROR)?
                    .to_str()?,
            )
            .ok_or(DESERIALIZE_ERROR)?,
            mime: response
                .headers()
                .get(CONTENT_TYPE)
                .ok_or(DESERIALIZE_ERROR)?
                .to_str()?
                .to_string(),
            response,
        })
    }

    // TODO, durable upload, dir ops, list ops
}
