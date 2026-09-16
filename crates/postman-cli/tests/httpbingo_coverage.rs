use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use postman_cli::parse_http_file;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CoverageManifest {
    schema_version: u8,
    source: String,
    statuses: BTreeMap<String, String>,
    endpoints: Vec<EndpointCoverage>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EndpointCoverage {
    family: String,
    status: String,
    suite: Option<String>,
    evidence: Option<String>,
    note: String,
}

#[test]
fn every_upstream_httpbingo_endpoint_family_has_a_reviewable_disposition() {
    let fixture_directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/httpbingo");
    let manifest_path = fixture_directory.join("coverage.json");
    let manifest: CoverageManifest = serde_json::from_str(
        &fs::read_to_string(&manifest_path).expect("coverage manifest should be readable"),
    )
    .expect("coverage manifest should be valid JSON");

    assert_eq!(manifest.schema_version, 1);
    assert!(
        manifest
            .source
            .contains("697b6e3b326edfd8915a3b1c36c4a43f367ea3ac/httpbin/httpbin.go"),
        "the endpoint inventory must cite a pinned upstream source revision"
    );
    assert_eq!(
        manifest
            .statuses
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["covered", "partial", "pending-model", "server-unavailable",])
    );
    assert!(manifest
        .statuses
        .values()
        .all(|description| !description.is_empty()));

    let expected_families = BTreeSet::from([
        "/",
        "/absolute-redirect/{n}",
        "/anything[/...path]",
        "/base64/{data}",
        "/base64/{operation}/{data}",
        "/basic-auth/{user}/{password}",
        "/bearer",
        "/brotli",
        "/bytes/{n}",
        "/cache",
        "/cache/{seconds}",
        "/cookies",
        "/cookies/delete",
        "/cookies/set",
        "/deflate",
        "/delay/{duration}",
        "/delete",
        "/deny",
        "/digest-auth/{qop}/{user}/{password}[/{algorithm}]",
        "/drip",
        "/dump/request",
        "/encoding/utf8",
        "/env",
        "/etag/{etag}",
        "/forms/post",
        "/get",
        "/gzip",
        "/head",
        "/headers",
        "/hidden-basic-auth/{user}/{password}",
        "/hostname",
        "/html",
        "/image[/{kind}]",
        "/ip",
        "/json",
        "/jsonl",
        "/links/{n}[/{offset}]",
        "/patch",
        "/post",
        "/put",
        "/range/{n}",
        "/redirect-to",
        "/redirect/{n}",
        "/relative-redirect/{n}",
        "/response-headers",
        "/robots.txt",
        "/sse",
        "/status/{code}",
        "/stream-bytes/{n}",
        "/stream/{n}",
        "/trailers",
        "/unstable",
        "/upload",
        "/user-agent",
        "/uuid",
        "/version",
        "/websocket/echo",
        "/xml",
    ]);
    let actual_families = manifest
        .endpoints
        .iter()
        .map(|endpoint| endpoint.family.as_str())
        .collect::<BTreeSet<_>>();

    assert_eq!(
        actual_families.len(),
        manifest.endpoints.len(),
        "families must be unique"
    );
    assert_eq!(actual_families, expected_families);

    let mut status_counts = BTreeMap::<&str, usize>::new();
    let mut parsed_suites = BTreeSet::new();
    let mut request_count = 0;
    for endpoint in &manifest.endpoints {
        assert!(
            !endpoint.note.trim().is_empty(),
            "{} needs a note",
            endpoint.family
        );
        *status_counts.entry(endpoint.status.as_str()).or_default() += 1;

        match endpoint.status.as_str() {
            "covered" | "partial" => {
                let suite = endpoint
                    .suite
                    .as_deref()
                    .unwrap_or_else(|| panic!("{} needs a suite", endpoint.family));
                let evidence = endpoint
                    .evidence
                    .as_deref()
                    .unwrap_or_else(|| panic!("{} needs evidence", endpoint.family));
                let suite_path = fixture_directory.join(suite);
                let source = fs::read_to_string(&suite_path).unwrap_or_else(|error| {
                    panic!("cannot read {}: {error}", suite_path.display())
                });
                assert!(
                    source.contains(evidence),
                    "{} does not contain evidence for {}: {evidence:?}",
                    suite_path.display(),
                    endpoint.family
                );
                if parsed_suites.insert(suite.to_owned()) {
                    let file = parse_http_file(&source).unwrap_or_else(|error| {
                        panic!("{} should parse: {error}", suite_path.display())
                    });
                    request_count += file.requests.len();
                }
            }
            "pending-model" | "server-unavailable" => {
                assert!(
                    endpoint.suite.is_none(),
                    "{} must not claim a suite",
                    endpoint.family
                );
                assert!(
                    endpoint.evidence.is_none(),
                    "{} must not claim executable evidence",
                    endpoint.family
                );
            }
            status => panic!(
                "unsupported coverage status `{status}` for {}",
                endpoint.family
            ),
        }
    }

    assert_eq!(status_counts.get("covered"), Some(&45));
    assert_eq!(status_counts.get("partial"), Some(&6));
    assert_eq!(status_counts.get("pending-model"), Some(&6));
    assert_eq!(status_counts.get("server-unavailable"), Some(&1));
    assert_eq!(parsed_suites.len(), 6);
    assert_eq!(request_count, 67);
}
