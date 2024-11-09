use crate::error::ConnectorError::Remote;
use crate::error::{ConnectorError, NodeClientError};

pub const DESERIALIZE_ERROR: ConnectorError = Remote(NodeClientError::BadRequest);

pub fn extract_filename(content_disposition: &str) -> Option<String> {
    if let Some(start) = content_disposition.find("filename=") {
        let filename_start = start + "filename=".len();
        let filename = &content_disposition[filename_start..];

        let filename = filename.trim_matches('"');
        return Some(filename.to_string());
    }
    None
}
