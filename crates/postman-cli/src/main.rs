use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use postman_cli::{parse_http_file, HeadlessRunner, HttpFile, RunReport};
use postman_http::request::{RedirectPolicy, RequestOptions};
use postman_request::RequestClient;
use serde::Serialize;

const USAGE: &str = "Usage: postman run <file-or-directory>... [--var name=value]... [--timeout-ms N] [--no-follow-redirects] [--json]";

struct RunArguments {
    paths: Vec<PathBuf>,
    variables: BTreeMap<String, String>,
    timeout_ms: Option<u64>,
    follow_redirects: bool,
    json: bool,
}

struct ParsedFile {
    path: PathBuf,
    file: HttpFile,
}

#[derive(Serialize)]
struct SuiteReport {
    schema_version: u8,
    success: bool,
    files: Vec<FileReport>,
}

#[derive(Serialize)]
struct FileReport {
    path: String,
    report: RunReport,
}

#[tokio::main]
async fn main() -> ExitCode {
    match execute(env::args().skip(1).collect()).await {
        Ok(success) if success => ExitCode::SUCCESS,
        Ok(_) => ExitCode::from(1),
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(2)
        }
    }
}

async fn execute(arguments: Vec<String>) -> Result<bool, String> {
    if arguments
        .iter()
        .any(|argument| matches!(argument.as_str(), "-h" | "--help"))
    {
        println!("{USAGE}");
        return Ok(true);
    }

    let arguments = parse_arguments(arguments)?;
    let paths = discover_http_files(&arguments.paths)?;
    let files = paths
        .into_iter()
        .map(|path| {
            let source = fs::read_to_string(&path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            let file =
                parse_http_file(&source).map_err(|error| format!("{}:{error}", path.display()))?;
            Ok(ParsedFile { path, file })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let client = RequestClient::try_new(concat!("postman-cli/", env!("CARGO_PKG_VERSION")))
        .map_err(|error| format!("cannot initialize HTTP transport: {error}"))?;
    let options = RequestOptions {
        timeout_ms: arguments.timeout_ms,
        redirect_policy: if arguments.follow_redirects {
            RedirectPolicy::Follow
        } else {
            RedirectPolicy::DoNotFollow
        },
        ..RequestOptions::default()
    };
    let runner = HeadlessRunner::new(client).with_options(options);
    let mut reports = Vec::with_capacity(files.len());
    for parsed in files {
        let report = runner.run(&parsed.file, &arguments.variables).await;
        reports.push(FileReport {
            path: parsed.path.display().to_string(),
            report,
        });
    }
    let report = SuiteReport {
        schema_version: 1,
        success: reports.iter().all(|file| file.report.success),
        files: reports,
    };

    if arguments.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|error| format!("cannot serialize run report: {error}"))?
        );
    } else {
        print_human_report(&report);
    }

    Ok(report.success)
}

fn parse_arguments(arguments: Vec<String>) -> Result<RunArguments, String> {
    let mut arguments = arguments.into_iter();
    match arguments.next().as_deref() {
        Some("run") => {}
        Some(command) => return Err(format!("unknown command `{command}`\n{USAGE}")),
        None => return Err(USAGE.to_owned()),
    }
    let mut paths = Vec::new();
    let mut variables = BTreeMap::new();
    let mut timeout_ms = None;
    let mut follow_redirects = true;
    let mut json = false;

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--var" => {
                let assignment = arguments
                    .next()
                    .ok_or_else(|| "`--var` requires `name=value`".to_owned())?;
                let (name, value) = assignment
                    .split_once('=')
                    .ok_or_else(|| "`--var` requires `name=value`".to_owned())?;
                if name.is_empty() {
                    return Err("`--var` name cannot be empty".to_owned());
                }
                variables.insert(name.to_owned(), value.to_owned());
            }
            "--timeout-ms" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "`--timeout-ms` requires an integer".to_owned())?;
                let value = value.parse::<u64>().map_err(|_| {
                    format!("invalid `--timeout-ms` value `{value}`: expected an integer")
                })?;
                if value == 0 {
                    return Err("`--timeout-ms` must be greater than zero".to_owned());
                }
                timeout_ms = Some(value);
            }
            "--no-follow-redirects" => follow_redirects = false,
            "--json" => json = true,
            option if option.starts_with('-') => {
                return Err(format!("unknown option `{option}`\n{USAGE}"))
            }
            path => paths.push(PathBuf::from(path)),
        }
    }

    if paths.is_empty() {
        return Err(format!("missing .http file or directory path\n{USAGE}"));
    }

    Ok(RunArguments {
        paths,
        variables,
        timeout_ms,
        follow_redirects,
        json,
    })
}

fn discover_http_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut files = BTreeSet::new();
    for path in paths {
        collect_http_files(path, &mut files)?;
    }
    if files.is_empty() {
        return Err("no .http files were found in the supplied paths".to_owned());
    }
    Ok(files.into_iter().collect())
}

fn collect_http_files(path: &Path, files: &mut BTreeSet<PathBuf>) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
    if metadata.is_file() {
        files.insert(path.to_path_buf());
        return Ok(());
    }
    if !metadata.is_dir() {
        return Err(format!(
            "{} is neither a file nor a directory",
            path.display()
        ));
    }

    let mut entries = fs::read_dir(path)
        .map_err(|error| format!("cannot read directory {}: {error}", path.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("cannot read directory {}: {error}", path.display()))?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let file_type = entry
            .file_type()
            .map_err(|error| format!("cannot inspect {}: {error}", entry.path().display()))?;
        if file_type.is_symlink() {
            continue;
        }
        let entry_path = entry.path();
        if file_type.is_dir() {
            collect_http_files(&entry_path, files)?;
        } else if entry_path
            .extension()
            .and_then(|extension| extension.to_str())
            == Some("http")
        {
            files.insert(entry_path);
        }
    }
    Ok(())
}

fn print_human_report(report: &SuiteReport) {
    for file in &report.files {
        println!("\n==> {}", file.path);
        for request in &file.report.requests {
            let marker = if request.success { "PASS" } else { "FAIL" };
            match (request.status, request.elapsed_ms) {
                (Some(status), Some(elapsed_ms)) => {
                    println!("{marker} {} — {status} ({elapsed_ms} ms)", request.name);
                }
                _ => println!("{marker} {}", request.name),
            }
            for assertion in &request.assertions {
                let assertion_marker = if assertion.success { "PASS" } else { "FAIL" };
                println!("  {assertion_marker} {}", assertion.expression);
                if let Some(message) = &assertion.message {
                    println!("    {message}");
                }
            }
            for capture in &request.captures {
                println!("  CAPTURE {capture}");
            }
            if let Some(error) = &request.error {
                println!("  {error}");
            }
        }
    }

    let passed = report
        .files
        .iter()
        .flat_map(|file| &file.report.requests)
        .filter(|request| request.success)
        .count();
    let total = report
        .files
        .iter()
        .map(|file| file.report.requests.len())
        .sum::<usize>();
    println!(
        "\n{}: {passed}/{total} request(s) passed across {} file(s)",
        if report.success { "PASS" } else { "FAIL" },
        report.files.len()
    );
}
