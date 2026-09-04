use reqwest::cookie::{CookieStore, Jar};
use reqwest::header::HeaderValue;
use reqwest::Url;
use std::collections::BTreeMap;
use std::future::Future;
use std::sync::{Arc, Mutex, RwLock};

tokio::task_local! {
    static RESPONSE_COOKIE_CAPTURE: Arc<Mutex<Vec<(String, String)>>>;
}

/// Reqwest's built-in Jar deliberately hides its contents. This wrapper keeps the wire behavior
/// delegated to that implementation while exposing only cookie names and origins to the UI. It
/// never copies sensitive values into application state or logs.
#[derive(Debug, Default)]
pub(crate) struct ApplicationCookieJar {
    jar: RwLock<Jar>,
    cookies_by_origin: RwLock<BTreeMap<String, Vec<String>>>,
}

impl ApplicationCookieJar {
    pub(crate) fn snapshot(&self) -> Vec<(String, String)> {
        self.cookies_by_origin
            .read()
            .expect("cookie snapshot lock should not be poisoned")
            .iter()
            .flat_map(|(origin, names)| {
                names.iter().map(move |name| (origin.clone(), name.clone()))
            })
            .collect()
    }

    pub(crate) fn clear(&self) -> usize {
        let mut jar = self
            .jar
            .write()
            .expect("cookie jar lock should not be poisoned");
        let mut snapshots = self
            .cookies_by_origin
            .write()
            .expect("cookie snapshot lock should not be poisoned");
        let cleared = snapshots.values().map(Vec::len).sum();
        snapshots.clear();
        *jar = Jar::default();
        cleared
    }

    fn refresh_origin_snapshot(&self, url: &Url, jar: &Jar) {
        let origin = cookie_origin(url);
        let mut names = jar
            .cookies(url)
            .and_then(|header| header.to_str().ok().map(str::to_string))
            .into_iter()
            .flat_map(|header| {
                header
                    .split(';')
                    .filter_map(|pair| pair.trim().split_once('=').map(|(name, _)| name.trim()))
                    .filter(|name| !name.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        names.sort();
        names.dedup();

        let mut snapshots = self
            .cookies_by_origin
            .write()
            .expect("cookie snapshot lock should not be poisoned");
        if names.is_empty() {
            snapshots.remove(&origin);
        } else {
            snapshots.insert(origin, names);
        }
    }
}

impl CookieStore for ApplicationCookieJar {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        let headers = cookie_headers.cloned().collect::<Vec<_>>();
        let origin = cookie_origin(url);
        let captured = headers
            .iter()
            .filter_map(|header| header.to_str().ok())
            .filter_map(set_cookie_name)
            .map(|name| (origin.clone(), name))
            .collect::<Vec<_>>();
        let _ = RESPONSE_COOKIE_CAPTURE.try_with(|capture| {
            capture
                .lock()
                .expect("response cookie capture lock should not be poisoned")
                .extend(captured);
        });
        let jar = self
            .jar
            .read()
            .expect("cookie jar lock should not be poisoned");
        jar.set_cookies(&mut headers.iter(), url);
        self.refresh_origin_snapshot(url, &jar);
    }

    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        self.jar
            .read()
            .expect("cookie jar lock should not be poisoned")
            .cookies(url)
    }
}

fn set_cookie_name(value: &str) -> Option<String> {
    value
        .split(';')
        .next()?
        .split_once('=')
        .map(|(name, _)| name.trim())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

fn cookie_origin(url: &Url) -> String {
    let host = url.host_str().unwrap_or("unknown-host");
    match url.port() {
        Some(port) => format!("{}://{host}:{port}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    }
}

pub(crate) async fn capture_response_cookies<F, T>(future: F) -> (T, Vec<(String, String)>)
where
    F: Future<Output = T>,
{
    let capture = Arc::new(Mutex::new(Vec::new()));

    let result = RESPONSE_COOKIE_CAPTURE.scope(capture.clone(), future).await;

    let mut stored_cookies = capture
        .lock()
        .expect("response cookie capture lock should not be poisoned")
        .clone();

    stored_cookies.sort();
    stored_cookies.dedup();

    (result, stored_cookies)
}
