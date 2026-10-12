use super::model::EnvironmentSet;
use std::{
    fs,
    io::{self, Write},
    path::Path,
};

pub(super) fn load(path: &Path) -> Result<(EnvironmentSet, Option<String>), String> {
    match fs::read(path) {
        Ok(bytes) => EnvironmentSet::decode(&bytes),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok((EnvironmentSet::default(), None)),
        Err(e) => Err(format!("Cannot read environments: {e}")),
    }
}

pub(super) fn save(path: &Path, value: &EnvironmentSet) -> Result<(), String> {
    let write = || -> io::Result<()> {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("Missing environment directory"))?;
        fs::create_dir_all(parent)?;
        let temporary = parent.join(format!(".environments-{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temporary)?;
            file.write_all(&serde_json::to_vec_pretty(value)?)?;
            file.sync_all()?;
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    };
    write().map_err(|e| {
        format!("Could not save environments: {e}. Your changes have not been applied. Try again.")
    })
}

#[cfg(test)]
mod tests {
    use super::super::model::Environment;
    use super::*;
    #[test]
    fn restart_corruption_and_write_failure() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("environments.json");
        assert_eq!(load(&path).unwrap().0, EnvironmentSet::default());
        let value = EnvironmentSet::draft(
            vec![Environment {
                id: "local".into(),
                name: "Local".into(),
                base_url: "http://localhost:3000".into(),
            }],
            Some("local".into()),
        );
        save(&path, &value).unwrap();
        assert_eq!(load(&path).unwrap().0, value);
        assert!(save(&path.join("invalid.json"), &value).is_err());
        assert_eq!(load(&path).unwrap().0, value);
        fs::write(&path, "broken").unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "broken");
        let missing = EnvironmentSet::draft(value.environments, Some("deleted".into()));
        save(&path, &missing).unwrap();
        let (recovered, warning) = load(&path).unwrap();
        assert_eq!(recovered.active_id.as_deref(), Some("local"));
        assert!(warning.is_some());
    }
}
