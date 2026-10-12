# cURL import and export

The native HTTP screen uses GPUI Kit dialogs matching the Import and Request code
surfaces in `prototypes/request-workspace.html`. Open **Import** from the HTTP rail,
or choose **Code → Import cURL**. Paste one literal POSIX command, inspect the live
preview, then choose **Import request**. This creates one unsent, dirty tab. It does
not replace another tab, apply the selected environment to the pasted URL, or send
a request. Escape, Cancel, the close button, and the backdrop discard the import.
Enter inserts a newline in the editor; use Tab and Space/Enter to activate a button.

**Code** snapshots the current effective request, including focused edits, using
Send's normalization and body validation. The selectable text is read-only. **Copy
code** copies exactly that text and reports success after checking the clipboard.
Opening, copying, or closing Code never saves the request or changes its dirty flag.
Explicit request credentials are included; the session cookie jar is not exported.
Neither feature executes cURL, reads referenced files, or performs network I/O.

**Try example** uses the same HTTPBingo JSON echo endpoint as the HTML prototype:

```sh
curl 'https://httpbingo.org/post' \
  --header 'Accept: application/json' \
  --json '{"name":"Maya Chen"}'
```

Sending it returns `200 OK`, with the submitted object under `json` in the response.

## Supported commands

| Area | Supported behavior |
| --- | --- |
| Shell syntax | Single/double quotes, adjacent quoted segments, escaped literal characters, backslash-LF/CRLF continuations; at most 256 KiB |
| Executable | `curl`, `curl.exe`, or a path ending in either name; arguments still use POSIX syntax |
| Arguments | Short flag groups, attached short values, `--option=value`, and `--` |
| URL | Exactly one absolute HTTP(S) URL; original query order, repeated keys, escapes, and fragment retained in the draft |
| Method | GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS; `-X/--request`, `-I/--head`, `-G/--get` and body-based inference |
| Headers | Ordered `-H/--header` fields, duplicates, `-A/--user-agent`, `-e/--referer`, and inline `-b/--cookie name=value` |
| Authorization | `-u/--user username:password`, `--oauth2-bearer`, explicit Authorization precedence; duplicate Authorization stays as independent Header rows |
| Inline bodies | `-d/--data`, `--data-raw`, inline `--data-binary`, `--data-urlencode`, `--json`; repeated data joins with `&`, repeated JSON fragments concatenate |
| Empty/literal data | An explicit empty body remains distinct from None; `--data-raw '@literal'` is literal data |
| Query data | `--get` adds encoded data after existing query text and before the fragment |
| Redirects | No flag means Do not follow; `-L/--location` or `--follow` enables native redirects; imported default limit is cURL's 50 hops |
| Limits | `-m/--max-time`: 0–86400 seconds, at most millisecond precision; zero disables timeout. `--max-redirs`: 1–100 |
| URL globbing | No expansion; quote the URL and use `-g/--globoff` for literal brackets/braces |
| Compression | Native decoding is already automatic. `--compressed` maps to that behavior; `Range` disables native decoding and cannot be combined with `--compressed` |
| Output flags | `-s/--silent`, `-S/--show-error`, `-v/--verbose`, `-i/--include`, `--no-progress-meter` affect terminal presentation only |
| Header suppression | `Content-Type:` and `Accept:` suppress generated defaults. Other suppression and forced empty `Name;` headers are rejected because the native editor cannot preserve them |

Valid JSON is shown in the JSON editor without reformatting its bytes. Canonically
encoded key/value bodies use the URL encoded editor; bodies whose percent encoding
or spacing would change are kept in Raw. Invalid JSON supplied to cURL also stays
Raw, retaining its Content-Type, because cURL itself does not validate JSON.
Explicit Content-Type and Accept headers take precedence over generated defaults.
Import supplies cURL's default `Accept: */*` unless explicitly overridden or
suppressed. `--json` supplies both JSON headers even with `--get`.

## Explicit boundaries and native adaptations

Unsupported options and conflicting modes produce an inline error and disable
Import; nothing is silently discarded. This includes shell variables/substitution,
command chaining, pipes/redirection, ANSI-C quoting, unmatched quotes, multiple
URLs, custom methods, URL credentials, and interactive password prompts. An
unquoted newline inside a command must be escaped with a continuation. Native
bodies are supported only for POST, PUT and PATCH: other methods with body data
must use `--get` or are rejected, including explicit empty bodies.

File/stdin bodies, header/cookie files, multipart imports, Windows CMD/PowerShell
syntax, proxy/certificate settings, and `--insecure` are outside this feature.
TLS verification remains enabled. No secret or command is logged by the converter.
Imported requests use the application's existing session cookies, User-Agent and
response decoding policy when subsequently sent; the preview summarizes the
explicit request fields, not hidden session state.

`--location` combined with `--request POST/PUT/PATCH/DELETE/OPTIONS` has redirect
method behavior that differs from the native client. Import rejects that combination
and explains `--follow`. Export lets cURL infer POST from its body/form options,
so ordinary POST, GET and HEAD requests can use `--location` on older versions.
Other custom methods (including POST without a body) use **`--follow` (requires
cURL 8.16+)** when Follow redirects is enabled, with a visible version requirement.
HEAD exports `--head` so cURL does not wait for a response body. Redirect limits and request timeouts are emitted explicitly.
See the official [cURL manual](https://curl.se/docs/manpage.html#--follow).

Compared with the JavaScript prototype, the native converter deliberately rejects
arbitrary method names, insecure TLS, unsupported body/method pairs, and header
forms the model cannot express. It persists actual request timeout/redirect
options instead of retaining inert cURL option strings. It preserves raw queries,
supports native Basic auth, and refuses conversions that lose header values or the
order of repeated fields. Managed auth can canonicalize the field-name casing and
move a single Authorization field relative to differently named headers.

## File export

Code handles None, JSON, Raw, URL encoded, Form-data and Binary drafts. Disabled
rows do not appear. Binary files export as `--data-binary '@path'`; multipart text
uses `--form-string`, while file parts use `--form` with separately quoted path,
filename and content-type metadata. Relative file paths are explicit filesystem
paths, including a literal file named `-`, rather than cURL's stdin sentinel.
Files are referenced, never opened or embedded by export. The dialog explains that
these commands cannot be imported by the inline-only importer.

A multipart file needs an explicit content type: the native transport omits it
when unset, whereas cURL guesses from the filename. Export reports this mismatch
instead of inventing metadata. Empty multipart bodies, field names containing
`=`/quotes/control characters, non-UTF-8 paths, parameterized multipart file media
types, and filenames containing newline/null characters also receive an error.
Invalid drafts disable Copy. This is intentionally bounded interchange, not a
complete shell or all of cURL's features.

## Implementation and verification

P4.3 baseline: `main` after #208,
`2a592fcc3768e952b0fb6e94c3afd7596ee18ebf` (GPUI Kit 0.7.1).

- `src/models/curl/`: pure tokenizer, fallible parser, draft projection and POSIX
  serializer. The parser never uses the fallback-to-GET conversion.
- `src/app/curl.rs`: retained Kit Textarea state, live preview and immutable Code
  snapshot, with one modal and the existing Root's focus/dismissal handling.
- `WorkspaceViewModel::import_request`: inserts a dirty tab without modifying
  history, environments, cookies or another tab's in-flight snapshot.
- `RequestDraft::from_request`: only promotes one canonical Authorization field;
  duplicates and noncanonical values remain lossless Header rows for replay too.

Conversion tests cover the prototype's shared inline subset, deliberate rejection
boundaries, semantic round trips through RequestDraft, Unicode/shell quoting,
file-reference generation without reading files, and disabled-row normalization.
`tests/ui_curl.rs` drives actual controls for import, latest edits, copy, cancellation,
Code-to-Import, keyboard focus, compact geometry and in-flight isolation. Controlled
loopback servers verify method, original query bytes, duplicate headers/auth, body,
redirect policy and subsequent History recording; preview and Copy send no traffic.

Visual checks compare the HTML and native surfaces at 1440×960 and 960×640 in
light/dark appearance: empty/valid/error import, long URL and multiline input,
Code overflow, file notes and focus. Native controls retain Kit focus treatment;
preview options and Code version/file explanations are native additions.

```bash
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
cargo build --locked
node --test prototypes/curl-request.test.cjs prototypes/request-body.test.cjs
```

Request Save/persistence remains P4.4; Flows and Add to flow are separate work.
