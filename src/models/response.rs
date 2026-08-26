/// One observed HTTP response in a redirect exchange.
///
/// Redirect responses retain the server's original `Location` value while the terminal response
/// has no location. URLs are the absolute request URLs used for each individual exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedirectHop {
    pub status: u16,
    pub url: String,
    pub location: Option<String>,
}

impl RedirectHop {
    pub fn new(status: u16, url: impl Into<String>, location: Option<impl Into<String>>) -> Self {
        Self {
            status,
            url: url.into(),
            location: location.map(Into::into),
        }
    }

    pub fn terminal(status: u16, url: impl Into<String>) -> Self {
        Self::new(status, url, None::<String>)
    }
}
