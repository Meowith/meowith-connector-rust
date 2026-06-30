pub fn extract_filename(content_disposition: &[u8]) -> Option<String> {
    let content_str = String::from_utf8_lossy(content_disposition);
    if let Some(start) = content_str.find("filename=") {
        let filename_start = start + "filename=".len();
        let filename = &content_str[filename_start..];

        let filename = filename.trim_matches('"');
        return Some(filename.to_string());
    }
    None
}
