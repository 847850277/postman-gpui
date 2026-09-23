use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

use postman_cli::{
    check_flow, compile_http_file, parse_http_file, run_flow_with_progress, HeadlessRunner,
    RunReport,
};
use postman_http::request::{RedirectPolicy, RequestOptions};
use postman_request::RequestClient;
use serde::Serialize;

const USAGE: &str = "Usage: postman-g run <file-or-directory>... [--var name=value]... [--input name=value]... [--check] [--timeout-ms N] [--no-follow-redirects] [--json] [-v|--verbose]";

struct RunArguments {
    paths: Vec<PathBuf>,
    variables: BTreeMap<String, String>,
    timeout_ms: Option<u64>,
    follow_redirects: bool,
    json: bool,
    check: bool,
    verbose: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum FileKind {
    Http,
    Flow,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DiscoveredFile {
    path: PathBuf,
    kind: FileKind,
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
    if arguments.verbose {
        let env_filter =
            tracing_subscriber::EnvFilter::new("postman_flow=debug,postman_cli=debug,info");
        let _ = tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(std::io::stderr)
            .try_init();
    } else if env::var_os("RUST_LOG").is_some() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .with_writer(std::io::stderr)
            .try_init();
    }
    let files = discover_files(&arguments.paths)?;

    if arguments.check {
        for file in &files {
            let source = fs::read_to_string(&file.path)
                .map_err(|error| format!("cannot read {}: {error}", file.path.display()))?;
            match file.kind {
                FileKind::Http => {
                    let parsed = parse_http_file(&source)
                        .map_err(|error| format!("{}:{error}", file.path.display()))?;
                    compile_http_file(&parsed)
                        .map_err(|error| format!("{}:{error}", file.path.display()))?;
                    println!(
                        "OK: {} ({} requests)",
                        file.path.display(),
                        parsed.requests.len()
                    );
                }
                FileKind::Flow => {
                    let check = check_flow(&source, &file.path)?;
                    println!("OK: {} ({} steps)", check.name, check.step_count);
                }
            }
        }
        return Ok(true);
    }

    let client = RequestClient::try_new(concat!("postman-g/", env!("CARGO_PKG_VERSION")))
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
    let runner = HeadlessRunner::new(client.clone()).with_options(options);
    let mut reports = Vec::with_capacity(files.len());
    for file in files {
        let source = fs::read_to_string(&file.path)
            .map_err(|error| format!("cannot read {}: {error}", file.path.display()))?;
        let report = match file.kind {
            FileKind::Http => {
                let parsed = parse_http_file(&source)
                    .map_err(|error| format!("{}:{error}", file.path.display()))?;
                runner
                    .run(&parsed, &arguments.variables)
                    .await
                    .map_err(|error| format!("{}:{error}", file.path.display()))?
            }
            FileKind::Flow => {
                let mut progress_started = false;
                run_flow_with_progress(
                    client.clone(),
                    &file.path,
                    &source,
                    &arguments.variables,
                    options,
                    |progress| {
                        if !arguments.json {
                            if !progress_started {
                                eprintln!("\n==> {}", file.path.display());
                                progress_started = true;
                            }
                            eprintln!("{progress}");
                        }
                    },
                )
                .await?
            }
        };
        reports.push(FileReport {
            path: file.path.display().to_string(),
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
    let mut check = false;
    let mut verbose = false;

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "-v" | "--verbose" => verbose = true,
            "--var" | "--input" => {
                let assignment = arguments
                    .next()
                    .ok_or_else(|| format!("`{argument}` requires `name=value`"))?;
                let (name, value) = assignment
                    .split_once('=')
                    .ok_or_else(|| format!("`{argument}` requires `name=value`"))?;
                if name.is_empty() {
                    return Err(format!("`{argument}` name cannot be empty"));
                }
                variables.insert(name.to_owned(), value.to_owned());
            }
            "--check" => check = true,
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
        return Err(format!(
            "missing .http or .http.yml file or directory path\n{USAGE}"
        ));
    }

    Ok(RunArguments {
        paths,
        variables,
        timeout_ms,
        follow_redirects,
        json,
        check,
        verbose,
    })
}

fn classify_file(path: &Path) -> Option<FileKind> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if name.ends_with(".http.yml")
        || name.ends_with(".http.yaml")
        || name.ends_with(".flow.yml")
        || name.ends_with(".flow.yaml")
    {
        Some(FileKind::Flow)
    } else if name.ends_with(".http") {
        Some(FileKind::Http)
    } else {
        None
    }
}

fn discover_files(paths: &[PathBuf]) -> Result<Vec<DiscoveredFile>, String> {
    let mut files = BTreeSet::new();
    for path in paths {
        collect_files(path, &mut files)?;
    }
    if files.is_empty() {
        return Err("no .http or .http.yml files were found in the supplied paths".to_owned());
    }
    Ok(files.into_iter().collect())
}

fn collect_files(path: &Path, files: &mut BTreeSet<DiscoveredFile>) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
    if metadata.is_file() {
        let kind = classify_file(path)
            .ok_or_else(|| format!("{} is not a .http or .http.yml file", path.display()))?;
        files.insert(DiscoveredFile {
            path: path.to_path_buf(),
            kind,
        });
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
            collect_files(&entry_path, files)?;
        } else if let Some(kind) = classify_file(&entry_path) {
            files.insert(DiscoveredFile {
                path: entry_path,
                kind,
            });
        }
    }
    Ok(())
}

fn print_human_report(report: &SuiteReport) {
    for file in &report.files {
        println!("\n==> {}", file.path);
        for request in &file.report.requests {
            let marker = if request.skipped {
                "SKIPPED"
            } else if request.success {
                "PASS"
            } else {
                "FAIL"
            };
            match (request.status, request.elapsed_ms) {
                (Some(status), Some(elapsed_ms)) => {
                    println!(
                        "{marker} {} — {status} ({elapsed_ms} ms)",
                        request.display_name()
                    );
                }
                _ => println!("{marker} {}", request.display_name()),
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
        for loop_report in &file.report.loops {
            println!("{}", loop_report.summary());
            for iteration in &loop_report.iterations {
                let marker = if iteration.success { "PASS" } else { "FAIL" };
                println!(
                    "  {marker} iteration {} ({} ms)",
                    iteration.iteration, iteration.elapsed_ms
                );
                if let Some(interval_ms) = iteration.wait_interval_ms {
                    println!(
                        "    WAIT {} ms (interval {interval_ms} ms)",
                        iteration.wait_elapsed_ms.unwrap_or(0)
                    );
                }
                if let Some(error) = &iteration.error {
                    println!("    {error}");
                }
            }
            for capture in &loop_report.captures {
                println!("  CAPTURE {capture}");
            }
            if let Some(error) = &loop_report.error {
                println!("  {error}");
            }
        }
        for (name, value) in &file.report.outputs {
            println!("  OUTPUT {name}: {value}");
        }
    }

    let passed = report
        .files
        .iter()
        .flat_map(|file| &file.report.requests)
        .filter(|request| request.success && !request.skipped)
        .count();
    let skipped = report
        .files
        .iter()
        .flat_map(|file| &file.report.requests)
        .filter(|request| request.skipped)
        .count();
    let total = report
        .files
        .iter()
        .map(|file| file.report.requests.len())
        .sum::<usize>();
    let loops = report
        .files
        .iter()
        .flat_map(|file| &file.report.loops)
        .collect::<Vec<_>>();
    let loop_summary = if loops.is_empty() {
        String::new()
    } else {
        let passed = loops
            .iter()
            .filter(|item| item.success && !item.skipped)
            .count();
        let skipped = loops.iter().filter(|item| item.skipped).count();
        format!(
            "; {passed}/{} loop(s) passed, {skipped} skipped",
            loops.len()
        )
    };
    if skipped > 0 {
        println!(
            "\n{}: {passed}/{total} request(s) passed, {skipped} skipped across {} file(s){loop_summary}",
            if report.success { "PASS" } else { "FAIL" },
            report.files.len()
        );
    } else {
        println!(
            "\n{}: {passed}/{total} request(s) passed across {} file(s){loop_summary}",
            if report.success { "PASS" } else { "FAIL" },
            report.files.len()
        );
    }
}
