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
    start: Option<i32>,
    end: Option<i32>,
}