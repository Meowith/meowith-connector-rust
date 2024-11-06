use crate::dto::request::RenameEntityRequest;
use crate::error::{ConnectorError, ConnectorResponse, NodeClientError};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_LENGTH};
use reqwest::{Body, Client, ClientBuilder};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use uuid::Uuid;

#[derive(Clone)]
struct MeowithConnector {
    client: Client,
    bucket_id: Uuid,
    app_id: Uuid,
    node_addr: String,
}

impl MeowithConnector {
    fn new(token: &str, bucket_id: Uuid, app_id: Uuid, node_addr: String) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(AUTHORIZATION, HeaderValue::from_str(format!("Bearer {}", token).as_str()).unwrap());

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

    async fn upload_oneshot(
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

    async fn delete_file(&self, path: &str) -> ConnectorResponse<()> {
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

    async fn rename_file(&self, from: &str, to: &str) -> ConnectorResponse<()> {
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

    async fn download_file(
        &self,
        stream: &mut (impl AsyncWrite + Unpin + Send),
        path: &str,
    ) -> ConnectorResponse<()> {
        let mut response = self
            .client
            .get(format!(
                "{}/api/file/download/{}/{}/{}",
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

        while let Some(chunk) = response.chunk().await? {
            stream.write_all(&chunk).await.unwrap()
        }

        Ok(())
    }

    // TODO, durable upload, dir ops, list ops
}
