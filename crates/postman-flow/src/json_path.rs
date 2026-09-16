use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JsonPath {
    source: String,
    segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Key(String),
    Index(usize),
}

impl JsonPath {
    pub fn compile(path: &str) -> Result<Self, String> {
        let mut remaining = path
            .strip_prefix('$')
            .ok_or_else(|| format!("JSONPath '{path}' must start with '$'"))?;
        let mut segments = Vec::new();
        while !remaining.is_empty() {
            if let Some(tail) = remaining.strip_prefix('.') {
                let end = tail.find(['.', '[']).unwrap_or(tail.len());
                let key = &tail[..end];
                if key.is_empty() || key.contains(['*', ']', '?']) {
                    return Err(format!("unsupported object key in JSONPath '{path}'"));
                }
                segments.push(Segment::Key(key.into()));
                remaining = &tail[end..];
            } else if let Some(tail) = remaining.strip_prefix('[') {
                let end = tail
                    .find(']')
                    .ok_or_else(|| format!("unterminated array index in JSONPath '{path}'"))?;
                let digits = &tail[..end];
                if digits.is_empty() || !digits.bytes().all(|value| value.is_ascii_digit()) {
                    return Err(format!("non-numeric array index in JSONPath '{path}'"));
                }
                let index = digits
                    .parse()
                    .map_err(|_| format!("array index is too large in JSONPath '{path}'"))?;
                segments.push(Segment::Index(index));
                remaining = &tail[end + 1..];
            } else {
                return Err(format!("unsupported JSONPath syntax in '{path}'"));
            }
        }
        Ok(Self {
            source: path.into(),
            segments,
        })
    }

    pub fn source(&self) -> &str {
        &self.source
    }

    pub fn resolve<'a>(&self, root: &'a Value) -> Result<&'a Value, String> {
        let mut current = root;
        for segment in &self.segments {
            current = match segment {
                Segment::Key(key) => current.get(key.as_str()),
                Segment::Index(index) => current.get(*index),
            }
            .ok_or_else(|| format!("JSONPath '{}' did not resolve in the response", self.source))?;
        }
        Ok(current)
    }
}
