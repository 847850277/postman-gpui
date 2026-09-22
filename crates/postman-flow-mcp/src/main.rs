use std::{env, path::PathBuf, process::ExitCode};

use postman_flow_mcp::FlowMcpServer;
use rmcp::{transport::stdio, ServiceExt};

const USAGE: &str = "Usage: postman-flow-mcp [--root DIRECTORY] [--max-steps N]";

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(1)
        }
    }
}

async fn run() -> Result<(), String> {
    let (root, max_steps) = parse_arguments(env::args().skip(1))?;
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .try_init();

    let server = FlowMcpServer::new(root, max_steps)?;
    let service = server
        .serve(stdio())
        .await
        .map_err(|error| format!("cannot start MCP stdio server: {error}"))?;
    service
        .waiting()
        .await
        .map_err(|error| format!("MCP server stopped with an error: {error}"))?;
    Ok(())
}

fn parse_arguments(
    arguments: impl IntoIterator<Item = String>,
) -> Result<(PathBuf, Option<usize>), String> {
    let mut arguments = arguments.into_iter();
    let mut root = None;
    let mut max_steps = None;
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--root" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--root requires a directory".to_owned())?;
                root = Some(PathBuf::from(value));
            }
            "--max-steps" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| "--max-steps requires an integer".to_owned())?;
                let value = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --max-steps value `{value}`"))?;
                if value == 0 {
                    return Err("--max-steps must be greater than zero".to_owned());
                }
                max_steps = Some(value);
            }
            "-h" | "--help" => return Err(USAGE.to_owned()),
            unknown => return Err(format!("unknown argument `{unknown}`\n{USAGE}")),
        }
    }
    let root = match root {
        Some(root) => root,
        None => {
            env::current_dir().map_err(|error| format!("cannot read current directory: {error}"))?
        }
    };
    Ok((root, max_steps))
}

#[cfg(test)]
mod tests {
    use super::parse_arguments;
    use std::path::PathBuf;

    #[test]
    fn parses_root_and_step_limit() {
        let (root, max_steps) = parse_arguments([
            "--root".to_owned(),
            "/tmp/flows".to_owned(),
            "--max-steps".to_owned(),
            "10".to_owned(),
        ])
        .unwrap();
        assert_eq!(root, PathBuf::from("/tmp/flows"));
        assert_eq!(max_steps, Some(10));
    }
}
