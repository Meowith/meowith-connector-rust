pub fn construct_pagination_query(range: Option<Range>) -> String {
    match range {
        None => String::new(),
        Some(range) => {
            if let (Some(start), Some(end)) = (range.start, range.end) {
                format!("?start={}&end={}", start, end)
            } else if let Some(start) = range.start {
                format!("?start={}-", start)
            } else if let Some(end) = range.end {
                format!("?end={}", end)
            } else {
                String::new()
            }
        }
    }
}

pub struct Range {
    pub start: Option<i32>,
    pub end: Option<i32>,
}

/// Bot start and end are inclusive.
pub struct DownloadRange {
    pub start: Option<u64>,
    pub end: Option<u64>,
}

impl DownloadRange {
    pub fn new(start: Option<u64>, end: Option<u64>) -> Self {
        Self { start, end }
    }

    pub fn full() -> Self {
        Self::new(None, None)
    }

    pub fn is_full(&self) -> bool {
        self.start.is_none() && self.end.is_none()
    }

    pub fn header_value(&self) -> String {
        match (self.start, self.end) {
            (Some(start), Some(end)) => format!("bytes={}-{}", start, end),
            (Some(start), None) => format!("bytes={}-", start),
            (None, Some(end)) => format!("bytes=-{}", end),
            (None, None) => String::from("bytes=0-"),
        }
    }
}
