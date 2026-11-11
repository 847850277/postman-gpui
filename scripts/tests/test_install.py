"""Offline installer integration tests: no real downloads or Cargo invocation."""
from __future__ import annotations

import hashlib
import io
import json
import os
import shlex
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path


INSTALLER = Path(__file__).resolve().parents[2] / "install.sh"
RELEASES = "https://github.com/847850277/postman-gpui/releases"
VERSION = "v1.2.3-rc.1"
PREBUILT = b"#!/bin/sh\necho 'prebuilt fixture'\n"
BUILT = b"#!/bin/sh\necho 'cargo fixture'\n"
OLD_BINARY = b"existing executable\n"

# One dispatcher for all mocked commands. PATH contains only these mocks and an
# explicit allowlist of filesystem/hash tools, so a missing mock cannot use the
# developer's downloader or Cargo installation.
MOCK_COMMAND = r'''
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

root = Path(os.environ["INSTALL_TEST_ROOT"])
config = json.loads((root / "config.json").read_text())
tool = Path(sys.argv[0]).name
args = sys.argv[1:]
call = {"tool": tool, "args": args}
if tool in ("curl", "wget"):
    call["url"] = args[-1]
with (root / "calls.jsonl").open("a") as stream:
    stream.write(json.dumps(call) + "\n")

if tool == "uname":
    print(config["os"] if args == ["-s"] else config["arch"])
elif tool in ("curl", "wget"):
    url = args[-1]
    base = config["releases"]
    if url == base + "/latest":
        kind = "release"
        headers = "HTTP/1.1 302 Found\r\nLocation: " + base + "/tag/" + config["latest_tag"] + "\r\n\r\n"
    elif url == base + "/tag/" + config["version"]:
        kind, headers = "release", ""
    elif url == base + "/download/" + config["version"] + "/" + config["asset"]:
        kind, headers = "asset", "HTTP/1.1 302 Found\r\nlocation: https://assets.example.test/archive\r\n\r\n"
    elif url == base + "/download/" + config["version"] + "/SHA256SUMS":
        kind, headers = "checksum", ""
    else:
        raise AssertionError("Unexpected URL: " + url)
    status = config.get(kind + "_status", 200)
    headers = "HTTP/1.1 200 Connection established\r\n\r\n" + headers
    headers += "HTTP/2 " + str(status) + " Response\r\n\r\n"
    if tool == "curl":
        assert "-sSL" in args and args[args.index("--write-out") + 1] == "%{http_code}"
        dest = Path(args[args.index("--output") + 1])
        Path(args[args.index("--dump-header") + 1]).write_text(headers)
        print(status, end="")
        code = 0
    else:
        assert "--server-response" in args and "--no-verbose" in args
        dest = Path(next(arg.split("=", 1)[1] for arg in args if arg.startswith("--output-document=")))
        sys.stderr.write("".join("  " + line + "\n" for line in headers.splitlines()))
        code = 0 if status == 200 else 8
    fixture = {"asset": "archive.tar.gz", "checksum": "SHA256SUMS"}.get(kind)
    if fixture and status == 200:
        shutil.copyfile(root / fixture, dest)
    else:
        dest.write_text("release page or error response")
    sys.exit(config.get(kind + "_exit", code))
elif tool == "cargo":
    prefix = Path(args[args.index("--root") + 1])
    (prefix / "bin").mkdir(parents=True)
    if not config.get("cargo_missing_binary"):
        (prefix / "bin/postman-g").write_bytes(config["built"].encode())
    sys.exit(config.get("cargo_exit", 0))
elif tool in ("tar", "install", "mv"):
    if config.get(tool + "_exit"):
        # Simulate a partially written staging file; the old binary must survive.
        if tool == "install":
            Path(args[-1]).write_text("incomplete copy")
        sys.exit(config[tool + "_exit"])
    sys.exit(subprocess.run([config["real_" + tool], *args]).returncode)
else:
    raise AssertionError("Unexpected command: " + tool)
'''


@unittest.skipIf(os.name == "nt", "install.sh targets macOS/Linux Bash")
class InstallerTests(unittest.TestCase):
    def run_installer(
        self, downloader="curl", *, version=VERSION, checksum=None,
        hasher=None, archive="valid", relative_dir=False, existing_binary=True, **overrides,
    ):
        temporary = tempfile.TemporaryDirectory(prefix="postman installer tests ")
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name).resolve()
        tools = root / "tools"
        tools.mkdir()
        install_dir = root / "custom bin"
        if existing_binary:
            install_dir.mkdir()
            (install_dir / "postman-g").write_bytes(OLD_BINARY)
            (install_dir / "postman-g").chmod(0o755)
        (root / "tmp").mkdir()
        home = root / "home"
        home.mkdir()
        config = {
            "releases": RELEASES, "version": version or VERSION, "latest_tag": VERSION,
            "os": "Linux", "arch": "x86_64", "built": BUILT.decode(), **overrides,
        }
        if config["os"] == "Darwin":
            asset = "postman-g-macos-universal.tar.gz"
        elif config["arch"] in ("aarch64", "arm64"):
            asset = "postman-g-linux-aarch64.tar.gz"
        else:
            asset = "postman-g-linux-x86_64.tar.gz"
        config["asset"] = asset
        for tool in ("awk", "mkdir", "mktemp", "rm", "gzip"):
            executable = shutil.which(tool)
            self.assertIsNotNone(executable, tool)
            (tools / tool).symlink_to(executable)
        if hasher != "none":
            hasher = hasher or ("sha256sum" if shutil.which("sha256sum") else "shasum")
            executable = shutil.which(hasher)
            if not executable:
                self.skipTest(f"{hasher} is not installed")
            # macOS shasum locates a versioned Perl script relative to argv[0],
            # so invoke its real path instead of placing a symlink in our PATH.
            hash_command = tools / hasher
            hash_command.write_text(f'#!/bin/bash\nexec {shlex.quote(executable)} "$@"\n')
            hash_command.chmod(0o755)
        mock = root / "mock-command"
        mock.write_text(f"#!{sys.executable}\n" + MOCK_COMMAND)
        mock.chmod(0o755)
        for tool in (downloader, "cargo", "uname", "tar", "install", "mv"):
            if tool in ("tar", "install", "mv"):
                config["real_" + tool] = shutil.which(tool)
            if tool == "cargo" and config.get("no_cargo"):
                continue
            if tool == "none":
                continue
            (tools / tool).symlink_to(mock)

        with tarfile.open(root / "archive.tar.gz", "w:gz") as tar:
            info = tarfile.TarInfo("other-file" if archive == "missing-binary" else "postman-g")
            info.mode = 0o755
            if archive == "symlink":
                info.type = tarfile.SYMTYPE
                info.linkname = str(install_dir / "postman-g")
                tar.addfile(info)
            else:
                info.size = len(PREBUILT)
                tar.addfile(info, io.BytesIO(PREBUILT))
        if archive == "corrupt":
            (root / "archive.tar.gz").write_bytes(b"not a tarball")
        digest = hashlib.sha256((root / "archive.tar.gz").read_bytes()).hexdigest()
        manifest = checksum(digest, asset) if checksum else f"{digest}  ./{asset}\n"
        (root / "SHA256SUMS").write_text(manifest)
        (root / "config.json").write_text(json.dumps(config))
        # Do not inherit Cargo, shell-startup, proxy, or downloader configuration.
        env = {
            "PATH": str(tools), "HOME": str(home), "TMPDIR": str(root / "tmp"),
            "INSTALL_TEST_ROOT": str(root), "LC_ALL": "C",
            "POSTMAN_INSTALL_DIR": "custom bin" if relative_dir else str(install_dir),
        }
        if version is not None:
            env["POSTMAN_VERSION"] = version
        result = subprocess.run(
            ["/bin/bash", str(INSTALLER)], cwd=root, env=env,
            capture_output=True, text=True, timeout=30,
        )
        calls_file = root / "calls.jsonl"
        calls = [json.loads(line) for line in calls_file.read_text().splitlines()] if calls_file.exists() else []
        self.assertEqual(list((root / "tmp").iterdir()), [], result.stderr)
        self.assertEqual(list(install_dir.glob(".postman-g.*")), [], result.stderr)
        self.assertFalse((home / ".cargo").exists(), "Must not install into Cargo's default root")
        return result, calls, install_dir

    def assert_success(self, result, install_dir, binary, version=VERSION):
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual((install_dir / "postman-g").read_bytes(), binary)
        self.assertEqual((install_dir / "postman-g").stat().st_mode & 0o777, 0o755)
        self.assertEqual([path.name for path in install_dir.iterdir()], ["postman-g"])
        self.assertIn(f"Successfully installed postman-g {version} to {install_dir}/postman-g", result.stdout)
        # The PATH hint must preserve spaces and defer expansion of the user's PATH.
        hint = next(line.strip() for line in result.stdout.splitlines() if line.startswith("  export PATH="))
        updated_path = subprocess.check_output(
            ["/bin/bash", "-c", hint + '; printf "%s" "$PATH"'],
            env={"PATH": "/existing/bin"}, text=True,
        )
        self.assertEqual(updated_path, f"{install_dir}:/existing/bin")

    def assert_failure(self, result, calls, install_dir, error, *, cargo=False, extracted=False):
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(error, result.stderr)
        self.assertNotIn("Successfully installed", result.stdout)
        self.assertEqual((install_dir / "postman-g").read_bytes(), OLD_BINARY)
        self.assertEqual(any(call["tool"] == "cargo" for call in calls), cargo)
        self.assertEqual(any(call["tool"] == "tar" for call in calls), extracted)
        self.assertFalse(any(call["tool"] == "install" for call in calls))

    def assert_pinned_cargo(self, calls):
        cargo = [call for call in calls if call["tool"] == "cargo"]
        self.assertEqual(len(cargo), 1)
        args = cargo[0]["args"]
        prefix = args[args.index("--root") + 1]
        self.assertEqual(args, [
            "install", "--git", "https://github.com/847850277/postman-gpui",
            "--tag", VERSION, "postman-cli", "--bin", "postman-g", "--locked", "--root", prefix,
        ])
        self.assertEqual(Path(prefix).name, "cargo")
        self.assertFalse(Path(prefix).exists(), "Temporary Cargo root must be cleaned up")

    def test_explicit_version_and_platform_assets(self):
        for downloader in ("curl", "wget"):
            for operating_system, arch, asset in (
                ("Linux", "x86_64", "postman-g-linux-x86_64.tar.gz"),
                ("Linux", "arm64", "postman-g-linux-aarch64.tar.gz"),
                ("Darwin", "arm64", "postman-g-macos-universal.tar.gz"),
            ):
                with self.subTest(downloader=downloader, os=operating_system, arch=arch):
                    result, calls, dest = self.run_installer(downloader, os=operating_system, arch=arch)
                    self.assert_success(result, dest, PREBUILT)
                    self.assertEqual([c["url"] for c in calls if "url" in c], [
                        f"{RELEASES}/tag/{VERSION}",
                        f"{RELEASES}/download/{VERSION}/{asset}",
                        f"{RELEASES}/download/{VERSION}/SHA256SUMS",
                    ])
                    self.assertFalse(any(c["tool"] == "cargo" for c in calls))

    def test_latest_resolves_once_for_binary_and_source(self):
        for downloader in ("curl", "wget"):
            for status in (200, 404):
                with self.subTest(downloader=downloader, asset_status=status):
                    result, calls, dest = self.run_installer(downloader, version=None, asset_status=status)
                    self.assert_success(result, dest, PREBUILT if status == 200 else BUILT)
                    urls = [c["url"] for c in calls if "url" in c]
                    self.assertEqual(urls.count(f"{RELEASES}/latest"), 1)
                    self.assertTrue(all(f"/download/{VERSION}/" in url for url in urls[1:]))
                    if status == 404:
                        self.assert_pinned_cargo(calls)

    def test_missing_asset_and_unsupported_architecture_use_pinned_source(self):
        for downloader in ("curl", "wget"):
            for options in ({"asset_status": 404}, {"arch": "riscv64"}):
                with self.subTest(downloader=downloader, **options):
                    result, calls, dest = self.run_installer(downloader, **options)
                    self.assert_success(result, dest, BUILT)
                    self.assert_pinned_cargo(calls)
                    self.assertFalse(any(c["tool"] == "tar" for c in calls))
                    if "arch" in options:
                        self.assertEqual([c["url"] for c in calls if "url" in c], [f"{RELEASES}/tag/{VERSION}"])

    def test_checksum_formats_and_both_hash_tools(self):
        for hasher in ("sha256sum", "shasum"):
            for marker in ("", "./", "*", "*./"):
                with self.subTest(hasher=hasher, marker=marker):
                    result, _, dest = self.run_installer(
                        hasher=hasher,
                        checksum=lambda digest, asset: f"{digest.upper()}  {marker}{asset}\n",
                    )
                    self.assert_success(result, dest, PREBUILT)

    def test_invalid_checksums_never_extract_or_fallback(self):
        manifests = {
            "empty": lambda digest, asset: "",
            "missing": lambda digest, asset: f"{digest}  another-asset\n",
            "invalid": lambda digest, asset: f"{'z' * 64}  {asset}\n",
            "short": lambda digest, asset: f"1234  {asset}\n",
            "duplicate": lambda digest, asset: f"{digest}  {asset}\n" * 2,
            "extra-field": lambda digest, asset: f"{digest}  {asset} extra\n",
            "mismatch": lambda digest, asset: f"{'0' * 64}  {asset}\n",
        }
        for downloader in ("curl", "wget"):
            for name, manifest in manifests.items():
                with self.subTest(downloader=downloader, manifest=name):
                    result, calls, dest = self.run_installer(downloader, checksum=manifest)
                    self.assert_failure(result, calls, dest, "SHA256")

    def test_http_errors_do_not_fallback(self):
        for downloader in ("curl", "wget"):
            for options, message in (
                ({"release_status": 404}, "Release"),
                ({"release_status": 404, "arch": "riscv64"}, "Release"),
                ({"release_status": 500}, "Release"),
                ({"release_status": 404, "version": None}, "latest release"),
                ({"asset_status": 403}, "Could not download"),
                ({"asset_status": 500}, "Could not download"),
                ({"checksum_status": 404}, "SHA256SUMS"),
                ({"checksum_status": 500}, "SHA256SUMS"),
            ):
                with self.subTest(downloader=downloader, **options):
                    result, calls, dest = self.run_installer(downloader, **options)
                    self.assert_failure(result, calls, dest, message)

    def test_transport_errors_including_incomplete_404_do_not_fallback(self):
        for downloader, exit_code in (("curl", 18), ("wget", 4)):
            for options in (
                {"release_exit": exit_code}, {"asset_exit": exit_code},
                {"asset_status": 404, "asset_exit": exit_code}, {"checksum_exit": exit_code},
            ):
                with self.subTest(downloader=downloader, **options):
                    result, calls, dest = self.run_installer(downloader, **options)
                    self.assert_failure(result, calls, dest, "download")

    def test_invalid_version_or_latest_redirect_stops_before_assets(self):
        for downloader in ("curl", "wget"):
            for options in (
                {"version": "latest"}, {"version": "v1.2.3/../../main"},
                {"version": "--main"}, {"version": None, "latest_tag": "main"},
            ):
                with self.subTest(downloader=downloader, **options):
                    result, calls, dest = self.run_installer(downloader, **options)
                    self.assert_failure(result, calls, dest, "Invalid release version")
                    self.assertFalse(any("/download/" in c.get("url", "") for c in calls))

    def test_latest_without_a_concrete_tag_fails(self):
        for downloader in ("curl", "wget"):
            with self.subTest(downloader=downloader):
                result, calls, dest = self.run_installer(downloader, version=None, latest_tag="")
                self.assert_failure(result, calls, dest, "did not resolve to a release tag")

    def test_bad_archives_do_not_fallback(self):
        for downloader in ("curl", "wget"):
            for archive in ("corrupt", "missing-binary", "symlink"):
                with self.subTest(downloader=downloader, archive=archive):
                    result, calls, dest = self.run_installer(downloader, archive=archive)
                    self.assert_failure(result, calls, dest, "Error:", extracted=True)

    def test_cargo_failure_or_missing_output_preserves_existing_binary(self):
        for downloader in ("curl", "wget"):
            for options, message in (
                ({"cargo_exit": 101}, "Cargo failed"),
                ({"cargo_missing_binary": True}, "executable is missing"),
            ):
                with self.subTest(downloader=downloader, **options):
                    result, calls, dest = self.run_installer(downloader, asset_status=404, **options)
                    self.assert_failure(result, calls, dest, message, cargo=True)
                    self.assert_pinned_cargo(calls)

    def test_missing_dependencies_fail_clearly(self):
        for options, message in (
            ({"downloader": "none"}, "curl or wget"),
            ({"hasher": "none"}, "sha256sum or shasum"),
            ({"asset_status": 404, "no_cargo": True}, "Rust/Cargo"),
        ):
            with self.subTest(**options):
                result, calls, dest = self.run_installer(**options)
                self.assert_failure(result, calls, dest, message)

    def test_staging_failure_preserves_existing_binary(self):
        for status in (200, 404):
            for tool in ("install", "mv"):
                with self.subTest(asset_status=status, tool=tool):
                    result, _, dest = self.run_installer(asset_status=status, **{tool + "_exit": 1})
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("Error:", result.stderr)
                    self.assertNotIn("Successfully installed", result.stdout)
                    self.assertEqual((dest / "postman-g").read_bytes(), OLD_BINARY)

    def test_relative_install_directory(self):
        for status in (200, 404):
            with self.subTest(asset_status=status):
                result, _, dest = self.run_installer(relative_dir=True, asset_status=status)
                self.assert_success(result, dest, PREBUILT if status == 200 else BUILT)

    def test_fresh_install_creates_the_destination(self):
        for downloader in ("curl", "wget"):
            for status in (200, 404):
                with self.subTest(downloader=downloader, asset_status=status):
                    result, _, dest = self.run_installer(downloader, asset_status=status, existing_binary=False)
                    self.assert_success(result, dest, PREBUILT if status == 200 else BUILT)


if __name__ == "__main__":
    unittest.main()
