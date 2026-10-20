//! Bounded native document I/O. Call only from the background executor.
use postman_flow::{parse_flow_yaml, write_flow_yaml, FlowDocument};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const MAX_DOCUMENT: u64 = 2 * 1024 * 1024;
const MAX_RECENTS: usize = 40;
const MAX_INDEX: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Recent {
    pub(super) path: PathBuf,
    pub(super) name: String,
}
#[derive(Serialize, Deserialize)]
struct Index {
    version: u32,
    files: Vec<Recent>,
}

pub(super) struct Loaded {
    pub(super) path: PathBuf,
    pub(super) bytes: Vec<u8>,
    pub(super) document: FlowDocument,
}
#[derive(Debug)]
pub(super) enum WriteError {
    Conflict(Option<Vec<u8>>),
    Failed(String),
}

pub(super) fn identity(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        if !path.is_file() {
            return Err("Choose a file, not a directory.".into());
        }
        fs::canonicalize(path).map_err(|e| e.to_string())
    } else {
        let name = path.file_name().ok_or("Choose a filename.")?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        Ok(fs::canonicalize(parent)
            .map_err(|e| format!("Cannot access the destination directory: {e}"))?
            .join(name))
    }
}
fn read_bounded(path: &Path, limit: u64) -> Result<Option<Vec<u8>>, String> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Cannot read {}: {e}", path.display())),
    };
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("Choose a regular file.".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err(format!(
            "{} exceeds the {} KiB limit.",
            path.display(),
            limit / 1024
        ));
    }
    Ok(Some(bytes))
}
pub(super) fn open(path: &Path) -> Result<Loaded, String> {
    let path = identity(path)?;
    let bytes = read_bounded(&path, MAX_DOCUMENT)?
        .ok_or("This file no longer exists. Choose it again or remove its recent entry.")?;
    let text = std::str::from_utf8(&bytes).map_err(|_| "Flow files must be UTF-8 YAML.")?;
    let document = parse_flow_yaml(text).map_err(|e| {
        if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("rs")) {
            format!("Open a Flow YAML document (.http.yml, .yml or .yaml). This selection appears to be Rust source; choose the matching YAML in crates/postman-flow/examples/flows/.\n{e}")
        } else {
            format!("Cannot open this Flow YAML document. Choose a Flow file (.http.yml, .yml or .yaml), or correct the YAML and try again.\n{e}")
        }
    })?;
    Ok(Loaded {
        path,
        bytes,
        document,
    })
}
pub(super) fn encode(document: &FlowDocument) -> Result<Vec<u8>, String> {
    write_flow_yaml(document)
        .map(String::into_bytes)
        .map_err(|e| e.to_string())
}
/// Compare the bytes read at Open/Save (or explicitly confirmed replacement) immediately
/// before replacement. This detects external edits; it is not a filesystem-wide lock.
pub(super) fn write(path: &Path, bytes: &[u8], expected: Option<&[u8]>) -> Result<(), WriteError> {
    let check = || {
        let actual = read_bounded(path, MAX_DOCUMENT).map_err(WriteError::Failed)?;
        if actual.as_deref() != expected {
            Err(WriteError::Conflict(actual))
        } else {
            Ok(())
        }
    };
    check()?;
    if fs::metadata(path).is_ok_and(|m| m.permissions().readonly()) {
        return Err(WriteError::Failed(
            "This file is readonly. Choose Save as to keep your changes elsewhere.".into(),
        ));
    }
    replace(path, bytes, check)
}
fn replace(
    path: &Path,
    bytes: &[u8],
    before_replace: impl FnOnce() -> Result<(), WriteError>,
) -> Result<(), WriteError> {
    let parent = path
        .parent()
        .ok_or_else(|| WriteError::Failed("Missing destination directory.".into()))?;
    let temporary = parent.join(format!(".postman-flow-{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temporary)
            .map_err(|e| WriteError::Failed(e.to_string()))?;
        file.write_all(bytes)
            .map_err(|e| WriteError::Failed(e.to_string()))?;
        // Keep the temporary file private while writing, then preserve the
        // destination's mode. New files stay 0600.
        #[cfg(unix)]
        match fs::metadata(path) {
            Ok(metadata) => file
                .set_permissions(metadata.permissions())
                .map_err(|e| WriteError::Failed(e.to_string()))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(WriteError::Failed(error.to_string())),
        }
        file.sync_all()
            .map_err(|e| WriteError::Failed(e.to_string()))?;
        drop(file);
        before_replace()?;
        fs::rename(&temporary, path).map_err(|e| WriteError::Failed(e.to_string()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
pub(super) fn load_recent(path: &Path) -> Result<Vec<Recent>, String> {
    let Some(bytes) = read_bounded(path, MAX_INDEX)? else {
        return Ok(Vec::new());
    };
    let index: Index =
        serde_json::from_slice(&bytes).map_err(|e| format!("Cannot read recent flows: {e}"))?;
    if index.version != 1 {
        return Err("This recent-flow index requires another application version.".into());
    }
    if index.files.len() > MAX_RECENTS {
        return Err("The recent-flow index exceeds its entry limit.".into());
    }
    let mut paths = std::collections::HashSet::new();
    if index
        .files
        .iter()
        .any(|file| !file.path.is_absolute() || !paths.insert(&file.path))
    {
        return Err("The recent-flow index contains invalid or duplicate file paths.".into());
    }
    Ok(index.files)
}
pub(super) fn remember(files: &mut Vec<Recent>, path: PathBuf, name: String) {
    files.retain(|f| f.path != path);
    files.insert(
        0,
        Recent {
            path,
            name: name.chars().take(80).collect(),
        },
    );
    files.truncate(MAX_RECENTS);
}
pub(super) fn save_recent(path: &Path, files: &[Recent]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(&Index {
        version: 1,
        files: files.to_vec(),
    })
    .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_INDEX {
        return Err(
            "The recent-flow index exceeds its size limit. Document files are unchanged.".into(),
        );
    }
    replace(path, &bytes, || Ok(())).map_err(|e| format!("Could not update recent flows: {e:?}"))
}

#[cfg(test)]
mod tests;
