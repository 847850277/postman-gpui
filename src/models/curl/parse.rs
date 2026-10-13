use super::{shell, validate_header, validate_url, CurlError, CurlImport, Result};
use crate::models::{
    HttpMethod, RedirectPolicy, Request, RequestBody, RequestDraft, RequestOptions,
    MAX_REDIRECT_HOPS,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};

const VALUES: &[&str] = &[
    "--request",
    "--url",
    "--header",
    "--data",
    "--data-raw",
    "--data-binary",
    "--data-urlencode",
    "--json",
    "--user",
    "--oauth2-bearer",
    "--user-agent",
    "--referer",
    "--cookie",
    "--max-time",
    "--max-redirs",
];

/// Parse one literal command into a validated native draft, without executing or reading files.
pub fn parse(command: &str) -> Result<CurlImport> {
    let words = shell::words(command)?;
    let executable = words
        .first()
        .map(|s| s.rsplit(['/', '\\']).next().unwrap_or(s));
    if !matches!(executable, Some("curl" | "curl.exe")) {
        return Err(CurlError::new(
            "Start with curl followed by one HTTP or HTTPS URL.",
        ));
    }
    let mut args = Vec::new();
    let mut iter = words.iter().skip(1);
    let mut end_options = false;
    while let Some(word) = iter.next() {
        if end_options || !word.starts_with('-') || word == "-" {
            args.push(("--url".to_string(), word.clone()));
            continue;
        }
        if word == "--" {
            end_options = true;
            continue;
        }
        let mut flags = Vec::new();
        if word.starts_with("--") {
            let (flag, value) = word
                .split_once('=')
                .map_or((word.as_str(), None), |(k, v)| (k, Some(v.to_string())));
            flags.push((flag.to_string(), value));
        } else {
            let mut shorts = word[1..].char_indices().peekable();
            while let Some((_, c)) = shorts.next() {
                let flag = match c {
                    'X' => "--request",
                    'H' => "--header",
                    'd' => "--data",
                    'u' => "--user",
                    'A' => "--user-agent",
                    'e' => "--referer",
                    'b' => "--cookie",
                    'm' => "--max-time",
                    'G' => "--get",
                    'I' => "--head",
                    'L' => "--location",
                    'k' => "--insecure",
                    's' => "--silent",
                    'S' => "--show-error",
                    'v' => "--verbose",
                    'i' => "--include",
                    'g' => "--globoff",
                    _ => return Err(CurlError::new(format!("Unsupported option: -{c}"))),
                };
                let value = if VALUES.contains(&flag) {
                    shorts.peek().map(|(i, _)| word[i + 1..].to_string())
                } else {
                    None
                };
                let takes_value = VALUES.contains(&flag);
                flags.push((flag.to_string(), value));
                if takes_value {
                    break;
                }
            }
        }
        for (flag, inline) in flags {
            let value = if VALUES.contains(&flag.as_str()) {
                inline
                    .or_else(|| iter.next().cloned())
                    .ok_or_else(|| CurlError::new(format!("Add a value for {flag}.")))?
            } else {
                if inline.is_some() {
                    return Err(CurlError::new(format!("{flag} does not take a value.")));
                }
                String::new()
            };
            args.push((flag, value));
        }
    }
    let mut request = Request::default();
    let mut options = RequestOptions {
        redirect_policy: RedirectPolicy::DoNotFollow,
        max_redirect_hops: 50,
        ..Default::default()
    };
    let (mut method, mut head, mut get, mut globoff) = (None, false, false, false);
    let (mut json, mut data) = (Vec::new(), Vec::new());
    let (mut basic, mut bearer) = (None, None);
    let mut suppressed = Vec::new();
    let mut follow = false;
    let mut compressed = false;
    for (flag, value) in args {
        match flag.as_str() {
            "--url" => {
                if !request.url.is_empty() {
                    return Err(CurlError::new("Import one URL at a time."));
                }
                validate_url(&value)?;
                request.url = value;
            }
            "--request" => {
                let parsed: HttpMethod = value.parse().map_err(CurlError::new)?;
                if parsed.to_string() != value {
                    return Err(CurlError::new("Use an uppercase supported HTTP method."));
                }
                if method.is_some_and(|old| old != parsed) {
                    return Err(CurlError::new("Use one unambiguous HTTP method."));
                }
                method = Some(parsed);
            }
            "--head" => head = true,
            "--get" => get = true,
            "--globoff" => globoff = true,
            "--location" => options.redirect_policy = RedirectPolicy::Follow,
            "--follow" => {
                follow = true;
                options.redirect_policy = RedirectPolicy::Follow;
            }
            "--max-time" => options.timeout_ms = timeout(&value)?,
            "--max-redirs" => {
                options.max_redirect_hops = value
                    .parse::<u32>()
                    .ok()
                    .filter(|v| (1..=MAX_REDIRECT_HOPS).contains(v))
                    .ok_or_else(|| {
                        CurlError::new("--max-redirs must be an integer from 1 to 100.")
                    })?;
            }
            "--header" => {
                if value.starts_with('@') {
                    return Err(CurlError::new(
                        "Header files are not supported. Paste inline headers.",
                    ));
                }
                if value.ends_with(';') && !value.contains(':') {
                    return Err(CurlError::new("Forced empty headers (Name;) are not supported by the native header editor."));
                }
                let (name, content) = value
                    .split_once(':')
                    .ok_or_else(|| CurlError::new("Write headers as 'Name: value'."))?;
                validate_header(name, content)?;
                if content.trim().is_empty() {
                    if !matches!(
                        name.to_ascii_lowercase().as_str(),
                        "content-type" | "accept"
                    ) {
                        return Err(CurlError::new("Only Content-Type and Accept default-header suppression can be represented. Forced empty headers are not supported."));
                    }
                    suppressed.push(name.to_string());
                } else {
                    request.add_header(name, content.trim_start_matches([' ', '\t']));
                }
            }
            "--user-agent" | "--referer" | "--cookie" => {
                let name = match flag.as_str() {
                    "--user-agent" => "User-Agent",
                    "--referer" => "Referer",
                    _ => "Cookie",
                };
                if flag == "--cookie" && !value.contains('=') {
                    return Err(CurlError::new(
                        "Cookie files are not supported. Paste inline name=value cookies.",
                    ));
                }
                if flag == "--referer" && value.ends_with(";auto") {
                    return Err(CurlError::new(
                        "Automatic Referer is not supported. Use a literal Referer header.",
                    ));
                }
                validate_header(name, &value)?;
                if value.trim().is_empty() {
                    return Err(CurlError::new("Empty shorthand headers are not supported."));
                }
                request.add_header(name, value);
            }
            "--user" => {
                if !value.contains(':') {
                    return Err(CurlError::new("Use --user 'username:password'; interactive password prompts are not supported."));
                }
                basic = Some(format!("Basic {}", STANDARD.encode(value.as_bytes())));
            }
            "--oauth2-bearer" => {
                if value.trim().is_empty() {
                    return Err(CurlError::new("Enter a bearer token."));
                }
                bearer = Some(format!("Bearer {value}"));
            }
            "--data" | "--data-raw" | "--data-binary" | "--data-urlencode" | "--json" => {
                if flag != "--data-raw" && value.starts_with('@') {
                    return Err(CurlError::new(
                        "File-based bodies and stdin are not imported. Paste inline data instead.",
                    ));
                }
                if flag == "--json" {
                    json.push(value);
                } else if flag == "--data-urlencode" {
                    data.push(urlencode(&value)?);
                } else {
                    data.push(value);
                }
            }
            // Presentation-only flags do not affect the native request. Decompression is
            // already automatic in RequestClient; globoff selects literal URLs below.
            "--silent" | "--show-error" | "--verbose" | "--include" | "--no-progress-meter" => {}
            "--compressed" => compressed = true,
            "--insecure" => {
                return Err(CurlError::new(
                    "--insecure is not supported. TLS certificate verification stays enabled.",
                ))
            }
            _ => return Err(CurlError::new(format!("Unsupported option: {flag}"))),
        }
    }
    if request.url.is_empty() {
        return Err(CurlError::new("Add an HTTP or HTTPS URL."));
    }
    // IPv6 authority brackets are not curl URL globbing.
    let after_host = request.url.split_once("://").unwrap().1;
    let path = after_host
        .find(['/', '?', '#'])
        .map_or("", |i| &after_host[i..]);
    if !globoff && (path.contains(['{', '}', '[', ']']) || after_host.contains(['{', '}'])) {
        return Err(CurlError::new(
            "URL globbing is not supported. Use --globoff for one literal URL.",
        ));
    }
    if !data.is_empty() && !json.is_empty() {
        return Err(CurlError::new(
            "Use either --json or --data options, not both.",
        ));
    }
    let has_body = !data.is_empty() || !json.is_empty();
    if head && (has_body && !get || method.is_some_and(|m| m != HttpMethod::HEAD)) {
        return Err(CurlError::new("--head conflicts with this method or body."));
    }
    request.method = method.unwrap_or(if head {
        HttpMethod::HEAD
    } else if get {
        HttpMethod::GET
    } else if has_body {
        HttpMethod::POST
    } else {
        HttpMethod::GET
    });
    if !follow
        && options.redirect_policy == RedirectPolicy::Follow
        && method.is_some_and(|m| !matches!(m, HttpMethod::GET | HttpMethod::HEAD))
    {
        return Err(CurlError::new("--location with a custom method keeps that method on redirects, which the native client cannot represent. Use --follow (cURL 8.16+) for native redirect semantics."));
    }
    let is_json = !json.is_empty();
    let body = if is_json {
        json.concat()
    } else {
        data.join("&")
    };
    if get {
        if !body.is_empty() {
            let (base, fragment) = request
                .url
                .split_once('#')
                .map_or((request.url.as_str(), None), |(b, f)| (b, Some(f)));
            let separator = if base.ends_with(['?', '&']) {
                ""
            } else if base.contains('?') {
                "&"
            } else {
                "?"
            };
            request.url = format!(
                "{base}{separator}{body}{}",
                fragment.map(|f| format!("#{f}")).unwrap_or_default()
            );
            validate_url(&request.url)?;
        }
    } else if has_body {
        if !request.method.allows_body() {
            return Err(CurlError::new("The native editor supports bodies only for POST, PUT and PATCH. Use --get to move data into the query."));
        }
        request.body = if serde_json::from_str::<serde_json::Value>(&body).is_ok() {
            RequestBody::Json(body.clone())
        } else {
            RequestBody::Raw(body.clone())
        };
        // Use the native URL-encoded editor only when it preserves the original bytes.
        if matches!(request.body, RequestBody::Raw(_)) && !is_json && body.contains('=') {
            let candidate = Request {
                body: RequestBody::UrlEncoded(body.clone()),
                ..request.clone()
            };
            if RequestDraft::from_request(&candidate)
                .construct()
                .request()
                .body
                == candidate.body
            {
                request.body = candidate.body;
            }
        }
        add_default(
            &mut request,
            &suppressed,
            "Content-Type",
            if is_json {
                "application/json"
            } else {
                "application/x-www-form-urlencoded"
            },
        );
    }
    if is_json {
        // --json supplies both headers even when --get moves its data to the URL.
        add_default(
            &mut request,
            &suppressed,
            "Content-Type",
            "application/json",
        );
    }
    add_default(
        &mut request,
        &suppressed,
        "Accept",
        if is_json { "application/json" } else { "*/*" },
    );
    if suppressed.iter().any(|name| {
        request
            .headers
            .iter()
            .any(|(h, _)| h.eq_ignore_ascii_case(name))
    }) {
        return Err(CurlError::new(
            "Do not mix suppression and values for the same header.",
        ));
    }
    if basic.is_some() && bearer.is_some() {
        return Err(CurlError::new(
            "Choose either Basic or bearer authentication.",
        ));
    }
    if let Some(auth) = bearer.or(basic) {
        add_default(&mut request, &suppressed, "Authorization", &auth);
    }
    if compressed
        && request
            .headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case("range"))
    {
        return Err(CurlError::new("--compressed with Range is not supported: the native transport preserves ranged response bytes without decoding."));
    }
    for (name, value) in &request.headers {
        validate_header(name, value)?;
    }
    CurlImport::new(request, options)
}
fn add_default(request: &mut Request, suppressed: &[String], name: &str, value: &str) {
    if !suppressed.iter().any(|n| n.eq_ignore_ascii_case(name))
        && !request
            .headers
            .iter()
            .any(|(n, _)| n.eq_ignore_ascii_case(name))
    {
        request.add_header(name, value);
    }
}
fn urlencode(value: &str) -> Result<String> {
    if let Some((name, value)) = value.split_once('=') {
        let encoded = percent_encode(value);
        Ok(if name.is_empty() {
            encoded
        } else {
            format!("{name}={encoded}")
        })
    } else if value.contains('@') {
        Err(CurlError::new(
            "File-based --data-urlencode is not supported.",
        ))
    } else {
        Ok(percent_encode(value))
    }
}
fn percent_encode(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(b) {
                (*b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn timeout(value: &str) -> Result<Option<u64>> {
    let bad =
        || CurlError::new("--max-time must be 0–86400 seconds with at most three decimal places.");
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|c| c.is_ascii_digit())
        || fraction.len() > 3
        || !fraction.bytes().all(|c| c.is_ascii_digit())
    {
        return Err(bad());
    }
    let seconds: u64 = whole.parse().map_err(|_| bad())?;
    let millis = seconds
        .checked_mul(1000)
        .and_then(|v| v.checked_add(format!("{fraction:0<3}").parse::<u64>().ok()?))
        .filter(|v| *v <= 86_400_000)
        .ok_or_else(bad)?;
    Ok((millis > 0).then_some(millis))
}
