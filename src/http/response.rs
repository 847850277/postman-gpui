pub struct HttpResponse {
    pub status_code: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl HttpResponse {
    pub fn new(status_code: u16, headers: Vec<(String, String)>, body: String) -> Self {
        HttpResponse {
            status_code,
            headers,
            body,
        }
    }

    pub fn status(&self) -> u16 {
        self.status_code
    }

    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn from_raw_response(raw_response: &str) -> Result<Self, &'static str> {
        let parts: Vec<&str> = raw_response.split("\r\n\r\n").collect();
        if parts.len() != 2 {
            return Err("Invalid raw response format");
        }

        let header_part = parts[0];
        let body_part = parts[1];

        let status_line = header_part.lines().next().ok_or("Missing status line")?;
        let status_code: u16 = status_line
            .split_whitespace()
            .nth(1)
            .ok_or("Missing status code")?
            .parse()
            .map_err(|_| "Invalid status code")?;

        let headers = header_part
            .lines()
            .skip(1)
            .filter_map(|line| {
                let mut parts = line.splitn(2, ": ");
                if let (Some(key), Some(value)) = (parts.next(), parts.next()) {
                    Some((key.to_string(), value.to_string()))
                } else {
                    None
                }
            })
            .collect();

        Ok(HttpResponse::new(
            status_code,
            headers,
            body_part.to_string(),
        ))
    }
}
