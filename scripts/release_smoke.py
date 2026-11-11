"""Controlled loopback fixtures and artifact-level CLI release smoke checks.

This is automated transport/Flow evidence, never desktop installation or IME evidence.
"""
from __future__ import annotations

import argparse
import base64
import contextlib
from datetime import datetime, timezone
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import platform
import re
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
from urllib.parse import parse_qs, urlsplit
import zipfile

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests/fixtures/flows"
UUID = "26100000-0000-4000-8000-000000000001"
MAX_BODY = 2 * 1024 * 1024


class SmokeError(RuntimeError):
    pass


def sha256(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


class FixtureServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, port: int = 0, log: Path | None = None):
        super().__init__(("127.0.0.1", port), FixtureHandler)
        self.records: list[dict] = []
        self.lock = threading.Lock()
        self.log = log
        self.health_requests = 0

    @property
    def url(self) -> str:
        return f"http://127.0.0.1:{self.server_port}"

    def capture(self, method: str, path: str, headers, body: bytes) -> dict:
        with self.lock:
            record = {
                "sequence": len(self.records) + 1, "method": method, "path": path,
                "headers": list(headers.items()), "body_base64": base64.b64encode(body).decode(),
                "body_sha256": hashlib.sha256(body).hexdigest(), "body_length": len(body),
                "body_utf8": body.decode("utf-8", errors="replace"),
            }
            self.records.append(record)
            if self.log:
                with self.log.open("a", encoding="utf-8") as stream:
                    stream.write(json.dumps(record, ensure_ascii=False) + "\n")
            return record


class FixtureHandler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, *_):
        pass

    def read_body(self) -> bytes:
        self.connection.settimeout(10)
        transfer = self.headers.get("Transfer-Encoding", "").lower()
        if transfer:
            if transfer != "chunked":
                raise ValueError("Only chunked Transfer-Encoding is supported")
            chunks, total = [], 0
            while True:
                line = self.rfile.readline(128)
                length = int(line.strip().split(b";", 1)[0], 16)
                if length < 0 or total + length > MAX_BODY:
                    raise ValueError("Body exceeds fixture limit")
                if length == 0:
                    # Consume trailers through the final empty line, with a bound.
                    for _ in range(100):
                        line = self.rfile.readline(8192)
                        if line == b"\r\n":
                            return b"".join(chunks)
                        if not line:
                            break
                    raise ValueError("Invalid chunk trailers")
                chunk = self.rfile.read(length)
                if len(chunk) != length or self.rfile.read(2) != b"\r\n":
                    raise ValueError("Incomplete chunk")
                chunks.append(chunk)
                total += length
        length = int(self.headers.get("Content-Length", "0"))
        if not 0 <= length <= MAX_BODY:
            raise ValueError("Invalid Content-Length")
        body = self.rfile.read(length)
        if len(body) != length:
            raise ValueError("Incomplete body")
        return body

    def respond(self, status: int, payload: dict, location: str | None = None):
        body = json.dumps(payload, ensure_ascii=False).encode()
        # Explicit HEAD + data in cURL must not wait for a nonexistent response body.
        if self.command == "HEAD":
            body = b""
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        if location:
            self.send_header("Location", location)
        self.end_headers()
        self.close_connection = True
        try:
            self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
            pass  # Expected when exercising cancellation/timeouts in the desktop.

    def handle_request(self):
        try:
            body = self.read_body()
        except (ValueError, OSError) as error:
            self.respond(400, {"error": str(error)})
            return
        record = self.server.capture(self.command, self.path, self.headers, body)
        url = urlsplit(self.path)
        args = parse_qs(url.query, keep_blank_values=True)
        payload = {"method": self.command, "args": args, "data": record["body_utf8"], "json": None}
        try:
            payload["json"] = json.loads(body)
        except (ValueError, UnicodeDecodeError):
            pass
        if url.path == "/uuid":
            payload["uuid"] = UUID
        elif url.path == "/anything/health":
            with self.server.lock:
                self.server.health_requests += 1
                ready = self.server.health_requests >= 2
            payload["args"] = {**args, "status": ["ready" if ready else "pending"]}
        elif url.path.startswith("/redirect/"):
            match = re.fullmatch(r"/redirect/(301|302|303|307|308)(/.*)", url.path)
            if not match:
                self.respond(400, {"error": "Use /redirect/<status>/<next-path>"})
                return
            location = match[2] + ("?" + url.query if url.query else "")
            self.respond(int(match[1]), payload, location)
            return
        elif url.path == "/delay":
            try:
                delay = float(args.get("seconds", ["2"])[0])
                if not 0 <= delay <= 10:
                    raise ValueError()
            except ValueError:
                self.respond(400, {"error": "seconds must be between 0 and 10"})
                return
            time.sleep(delay)
        elif url.path.startswith("/status/"):
            try:
                status = int(url.path.removeprefix("/status/"))
                if not 200 <= status <= 599:
                    raise ValueError()
            except ValueError:
                self.respond(400, {"error": "status must be between 200 and 599"})
                return
            self.respond(status, payload)
            return
        self.respond(200, payload)

    do_GET = do_POST = do_PUT = do_PATCH = do_DELETE = do_HEAD = do_OPTIONS = handle_request


@contextlib.contextmanager
def running_server():
    with FixtureServer() as server:
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            yield server
        finally:
            server.shutdown()
            thread.join()


def verify_checksum(artifact: Path, checksums: Path) -> None:
    entries = []
    for line in checksums.read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[1].removeprefix("*").removeprefix("./") == artifact.name:
            if len(parts) != 2 or not re.fullmatch(r"[0-9a-fA-F]{64}", parts[0]):
                raise SmokeError(f"Malformed checksum entry for {artifact.name}")
            entries.append(parts[0].lower())
    if len(entries) != 1 or entries[0] != sha256(artifact):
        raise SmokeError(f"Missing, duplicate, or mismatched SHA256 for {artifact.name}")


def read_build_info(path: Path, checksums: Path) -> dict:
    verify_checksum(path, checksums)
    values = {}
    for line in path.read_text(encoding="utf-8-sig").splitlines():
        key, separator, value = line.partition("=")
        if not separator or key in values:
            raise SmokeError("Malformed or duplicate BUILD-INFO field")
        values[key] = value
    required = ("tag", "commit", "workflow_run", "target", "signed", "notarized")
    if any(not values.get(key) for key in required) or not re.fullmatch(r"[0-9a-f]{40}", values["commit"]):
        raise SmokeError("BUILD-INFO must identify the full source commit and release workflow")
    return {**values, "sha256": sha256(path)}


def extract_cli(archive: Path, checksums: Path, destination: Path) -> Path:
    verify_checksum(archive, checksums)
    name = "postman-g.exe" if archive.suffix == ".zip" else "postman-g"
    # Read only the expected regular-file member, never extract arbitrary paths.
    if archive.suffix == ".zip":
        with zipfile.ZipFile(archive) as container:
            matches = [info for info in container.infolist() if info.filename == name]
            if len(matches) != 1 or matches[0].is_dir():
                raise SmokeError("CLI archive must contain exactly one executable")
            data = container.read(matches[0])
    else:
        with tarfile.open(archive, "r:gz") as container:
            matches = [info for info in container.getmembers() if info.name == name]
            if len(matches) != 1 or not matches[0].isfile():
                raise SmokeError("CLI archive must contain exactly one regular executable")
            with container.extractfile(matches[0]) as stream:
                data = stream.read()
    binary = destination / name
    binary.write_bytes(data)
    binary.chmod(0o755)
    return binary


def run_cli(binary: Path, fixture: Path, server: FixtureServer, check: bool = False) -> dict:
    command = [str(binary), "run", str(fixture), "--input", "base_url=" + server.url, "--json"]
    if check:
        command.append("--check")
    result = subprocess.run(command, capture_output=True, encoding="utf-8", timeout=45)
    if result.returncode:
        raise SmokeError(f"CLI failed ({fixture.name}, check={check}): {result.stderr or result.stdout}")
    if check:
        # Check reports diagnostics as text even when --json is present.
        return {"success": True, "diagnostic": result.stdout.strip()}
    try:
        report = json.loads(result.stdout)
    except ValueError as error:
        raise SmokeError(f"CLI did not return JSON for {fixture.name}") from error
    if report.get("success") is not True:
        raise SmokeError(f"CLI reported failure for {fixture.name}")
    return report


def body_matrix(directory: Path) -> tuple[Path, list[tuple[str, str, bytes]]]:
    payload = b"release smoke \x00\xff\r\n" + "中文".encode()
    (directory / "upload.bin").write_bytes(payload)
    (directory / "empty.bin").write_bytes(b"")
    cases = [
        ("none", None, b""),
        ("empty", {"kind": "raw", "value": {"literal": ""}}, b""),
        ("raw", {"kind": "raw", "value": {"literal": "body 中文\n"}}, "body 中文\n".encode()),
        ("file", {"kind": "file", "value": {"literal": str(directory / "upload.bin")}}, payload),
        ("empty-file", {"kind": "file", "value": {"literal": str(directory / "empty.bin")}}, b""),
    ]
    steps, expected = [], []
    for method in ("DELETE", "GET", "HEAD", "OPTIONS", "POST", "PUT", "PATCH"):
        for kind, body, sent in cases:
            path = f"/echo?case={method.lower()}-{kind}"
            request = {"kind": "http", "method": method, "url": {"concat": [{"input": "base_url"}, {"literal": path}]}}
            if body is not None:
                request["body"] = body
            steps.append({"id": f"{method.lower()}-{kind}", "request": request, "checks": [{"kind": "status", "equals": 200}]})
            expected.append((method, path, sent))
    document = {"schema_version": 1, "flow": {"name": "Release body matrix", "inputs": [{"name": "base_url"}], "steps": steps, "outputs": []}}
    fixture = directory / "body-matrix.http.yml"
    fixture.write_text(json.dumps(document), encoding="utf-8")
    return fixture, expected


def observed(records: list[dict]) -> list[tuple[str, str, bytes]]:
    return [(r["method"], r["path"], base64.b64decode(r["body_base64"])) for r in records]


def check_cli(binary: Path, directory: Path) -> dict:
    matrix, expected = body_matrix(directory)
    with running_server() as server:
        fixtures = [matrix, FIXTURES / "echo-chain.http.yml", FIXTURES / "poll-status.http.yml"]
        reports = []
        for fixture in fixtures:
            before = len(server.records)
            run_cli(binary, fixture, server, check=True)
            if len(server.records) != before:
                raise SmokeError(f"Check sent network traffic: {fixture.name}")
            reports.append({"fixture": fixture.name, "sha256": sha256(fixture), "check_no_requests": True,
                            "result": run_cli(binary, fixture, server)})
        records = server.records
        if observed(records[:len(expected)]) != expected:
            raise SmokeError("Body matrix received different methods, paths, or bytes")
        chain = records[len(expected):len(expected) + 3]
        if [(r["method"], r["path"]) for r in chain] != [("GET", "/uuid"), ("POST", "/post"), ("PATCH", "/patch")]:
            raise SmokeError("Echo chain sent unexpected requests")
        if json.loads(chain[1]["body_utf8"]) != {"id": UUID, "name": "Maya Chen"} or json.loads(chain[2]["body_utf8"]) != {"id": UUID, "status": "active"}:
            raise SmokeError("Echo chain did not propagate typed values")
        polling = records[len(expected) + 3:]
        if [(r["method"], r["path"]) for r in polling] != [("GET", "/anything/health?status=ready")] * 2:
            raise SmokeError("Polling did not stop after pending then ready")
        return {"success": True, "body_cases": len(expected), "requests": records, "flows": reports}


def create_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    serve = commands.add_parser("serve", help="loopback fixture for native desktop acceptance")
    serve.add_argument("--port", type=int, default=8765)
    serve.add_argument("--log", type=Path, required=True, help="new JSONL path for synthetic request evidence")
    cli = commands.add_parser("cli", help="verify actual CLI bytes against controlled body/Flow fixtures")
    source = cli.add_mutually_exclusive_group(required=True)
    source.add_argument("--binary", type=Path)
    source.add_argument("--archive", type=Path)
    cli.add_argument("--checksums", type=Path, help="required for archive input")
    cli.add_argument("--build-info", type=Path, help="checksum-verified BUILD-INFO for the same archive/platform")
    cli.add_argument("--output", type=Path, required=True)
    return parser


def main(argv: list[str] | None = None) -> int:
    args = create_parser().parse_args(argv)
    try:
        if args.command == "serve":
            # Refuse to mix new actions into an earlier acceptance session.
            with args.log.open("x", encoding="utf-8"):
                pass
            with FixtureServer(args.port, args.log) as server:
                print(f"Fixture listening on {server.url}; requests: {args.log.resolve()}", flush=True)
                try:
                    server.serve_forever()
                except KeyboardInterrupt:
                    pass
            return 0
        report = {
            "scope": "automated-cli-transport-and-flow", "native_desktop_acceptance": "not_run",
            "recorded_at": datetime.now(timezone.utc).isoformat(),
            "platform": platform.platform(), "architecture": platform.machine(), "success": False,
        }
        try:
            with tempfile.TemporaryDirectory(prefix="postman release smoke ") as temporary:
                directory = Path(temporary)
                if args.archive:
                    if not args.checksums:
                        raise SmokeError("--archive requires --checksums before extraction")
                    report["archive"] = {"filename": args.archive.name, "sha256": sha256(args.archive)}
                    binary = extract_cli(args.archive, args.checksums, directory)
                    if args.build_info:
                        report["build"] = read_build_info(args.build_info, args.checksums)
                else:
                    if args.build_info:
                        raise SmokeError("--build-info requires a verified --archive")
                    binary = args.binary.resolve()
                report["binary"] = {"filename": binary.name, "sha256": sha256(binary)}
                report.update(check_cli(binary, directory))
        except (SmokeError, OSError, ValueError, subprocess.SubprocessError, tarfile.TarError, zipfile.BadZipFile) as error:
            report["error"] = str(error)
        # Integrity failures must replace any previous success report too.
        args.output.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        if not report["success"]:
            raise SmokeError(report["error"])
        print(f"CLI smoke passed: {report['body_cases']} body cases, 2 Flow fixtures; evidence: {args.output}")
        return 0
    except (SmokeError, OSError, ValueError, tarfile.TarError, zipfile.BadZipFile) as error:
        print(f"release smoke error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
