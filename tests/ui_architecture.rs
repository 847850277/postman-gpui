use std::{
    fs,
    path::{Path, PathBuf},
};

const FORBIDDEN_APPLICATION_PATHS: [&str; 2] = ["crate::app", "app::"];

#[test]
fn ui_layer_does_not_depend_on_application_features() {
    let manifest_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let ui_root = manifest_root.join("src/ui");
    let mut rust_sources = Vec::new();
    collect_rust_sources(&ui_root, &mut rust_sources);
    rust_sources.sort();

    let mut violations = Vec::new();
    for path in rust_sources {
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let relative_path = path.strip_prefix(manifest_root).unwrap_or(&path);

        for (line_index, line) in source.lines().enumerate() {
            if FORBIDDEN_APPLICATION_PATHS
                .iter()
                .any(|forbidden| line.contains(forbidden))
            {
                violations.push(format!(
                    "{}:{}: {}",
                    relative_path.display(),
                    line_index + 1,
                    line.trim()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "the reusable UI layer must not depend on application features:\n{}",
        violations.join("\n")
    );
}

fn collect_rust_sources(directory: &Path, sources: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", directory.display()));

    for entry in entries {
        let path = entry
            .unwrap_or_else(|error| {
                panic!(
                    "failed to inspect an entry in {}: {error}",
                    directory.display()
                )
            })
            .path();

        if path.is_dir() {
            collect_rust_sources(&path, sources);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            sources.push(path);
        }
    }
}
