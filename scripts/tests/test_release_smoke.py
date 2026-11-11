from __future__ import annotations

import base64
import contextlib
import hashlib
import http.client
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import textwrap
import unittest
from unittest import mock
import zipfile


SCRIPT = Path(__file__).resolve().parents[1] / "release_smoke.py"
SPEC = importlib.util.spec_from_file_location("release_smoke", SCRIPT)
smoke = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(smoke)


class FixtureTests(unittest.TestCase):
    def request(self, server, method, path, body=None, headers=None, *, chunked=False):
        connection = http.client.HTTPConnection("127.0.0.1", server.server_port, timeout=5)
        try:
            connection.request(method, path, body=body, headers=headers or {}, encode_chunked=chunked)
            response = connection.getresponse()
            return response.status, dict(response.getheaders()), response.read()
        finally:
            connection.close()

    def test_records_exact_bytes_for_all_methods_including_head(self):
        with smoke.running_server() as server:
            for method in ("DELETE", "GET", "HEAD", "OPTIONS", "POST", "PUT", "PATCH"):
                for body in (b"", b"file\x00\xff\r\n", "你好".encode()):
                    with self.subTest(method=method, body=body):
                        status, headers, response = self.request(server, method, "/echo?x=1", body)
                        self.assertEqual(status, 200)
                        record = server.records[-1]
                        self.assertEqual(record["method"], method)
                        self.assertEqual(base64.b64decode(record["body_base64"]), body)
                        self.assertEqual(record["body_sha256"], hashlib.sha256(body).hexdigest())
                        self.assertEqual(record["body_length"], len(body))
                        if method == "HEAD":
                            self.assertEqual(response, b"")
                            self.assertEqual(headers["Content-Length"], "0")

    def test_chunked_upload_records_decoded_binary_bytes(self):
        with smoke.running_server() as server:
            status, _, _ = self.request(server, "HEAD", "/echo", [b"one", b"\x00\xfftwo"], chunked=True)
            self.assertEqual(status, 200)
            self.assertEqual(base64.b64decode(server.records[-1]["body_base64"]), b"one\x00\xfftwo")

    def test_redirects_preserve_query_and_record_each_observed_hop(self):
        with smoke.running_server() as server:
            for status in (301, 302, 303, 307, 308):
                with self.subTest(status=status):
                    code, headers, _ = self.request(server, "DELETE", f"/redirect/{status}/echo?case=hop", b"payload")
                    self.assertEqual(code, status)
                    self.assertEqual(headers["Location"], "/echo?case=hop")
                    self.request(server, "GET", headers["Location"])
                    self.assertEqual(smoke.observed(server.records[-2:]), [
                        ("DELETE", f"/redirect/{status}/echo?case=hop", b"payload"),
                        ("GET", "/echo?case=hop", b""),
                    ])

    def test_existing_flow_fixtures_receive_expected_echo_and_polling_responses(self):
        with smoke.running_server() as server:
            _, _, response = self.request(server, "GET", "/uuid")
            self.assertEqual(json.loads(response)["uuid"], smoke.UUID)
            _, _, response = self.request(server, "POST", "/post", b'{"id":"known"}')
            self.assertEqual(json.loads(response)["json"], {"id": "known"})
            for expected in ("pending", "ready", "ready"):
                _, _, response = self.request(server, "GET", "/anything/health?status=ready")
                self.assertEqual(json.loads(response)["args"]["status"], [expected])

    def test_rejects_bad_lengths_and_oversized_payloads_without_recording_success(self):
        with smoke.running_server() as server:
            for length in ("-1", "invalid", str(smoke.MAX_BODY + 1)):
                with self.subTest(length=length):
                    code, _, _ = self.request(server, "POST", "/echo", headers={"Content-Length": length})
                    self.assertEqual(code, 400)
            self.assertEqual(server.records, [])

    def test_jsonl_records_each_request_and_refuses_reusing_an_old_log(self):
        with tempfile.TemporaryDirectory() as temporary, smoke.running_server() as server:
            log = Path(temporary) / "requests.jsonl"
            server.log = log
            self.request(server, "POST", "/echo", "中文".encode())
            self.assertEqual(json.loads(log.read_text())["body_utf8"], "中文")
            self.assertEqual(smoke.main(["serve", "--log", str(log)]), 1)
            self.assertEqual(len(log.read_text().splitlines()), 1)


class ArchiveTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.output = self.root / "unpacked"
        self.output.mkdir()
        self.checksums = self.root / "SHA256SUMS"

    def make_archive(self, kind="tar", name=None, symlink=False):
        archive = self.root / ("cli.zip" if kind == "zip" else "cli.tar.gz")
        name = name or ("postman-g.exe" if kind == "zip" else "postman-g")
        if kind == "zip":
            with zipfile.ZipFile(archive, "w") as container:
                container.writestr(name, b"executable fixture")
        else:
            with tarfile.open(archive, "w:gz") as container:
                member = tarfile.TarInfo(name)
                if symlink:
                    member.type = tarfile.SYMTYPE
                    member.linkname = "../outside"
                    container.addfile(member)
                else:
                    member.size = len(b"executable fixture")
                    container.addfile(member, io.BytesIO(b"executable fixture"))
        self.checksums.write_text(f"{smoke.sha256(archive)}  ./{archive.name}\n")
        return archive

    def test_extracts_only_the_verified_cli_for_tar_and_zip(self):
        for kind in ("tar", "zip"):
            with self.subTest(kind=kind):
                archive = self.make_archive(kind)
                binary = smoke.extract_cli(archive, self.checksums, self.output)
                self.assertEqual(binary.read_bytes(), b"executable fixture")

    def test_invalid_checksum_never_extracts(self):
        for manifest in ("", "bad  ./cli.tar.gz\n", "0" * 64 + "  ./cli.tar.gz\n", "duplicate"):
            with self.subTest(manifest=manifest):
                archive = self.make_archive()
                self.checksums.write_text(self.checksums.read_text() * 2 if manifest == "duplicate" else manifest)
                with self.assertRaises(smoke.SmokeError):
                    smoke.extract_cli(archive, self.checksums, self.output)
                self.assertEqual(list(self.output.iterdir()), [])

    def test_missing_executable_and_symlink_are_rejected(self):
        for options in ({"name": "../outside"}, {"symlink": True}, {"kind": "zip", "name": "../outside"}):
            with self.subTest(**options):
                archive = self.make_archive(**options)
                with self.assertRaises(smoke.SmokeError):
                    smoke.extract_cli(archive, self.checksums, self.output)
                self.assertEqual(list(self.output.iterdir()), [])
                self.assertFalse((self.root / "outside").exists())

    def test_build_identity_requires_verified_complete_metadata(self):
        metadata = self.root / "BUILD-INFO-windows.txt"
        content = ("tag=v1.2.3-rc.1\ncommit=" + "a" * 40 +
                   "\nworkflow_run=https://github.com/example/repo/actions/runs/1/attempts/2\n"
                   "target=x86_64-pc-windows-msvc\nsigned=false\nnotarized=n/a\n")
        metadata.write_text(content, encoding="utf-8-sig", newline="\r\n")
        self.checksums.write_text(f"{smoke.sha256(metadata)}  ./{metadata.name}\n")
        identity = smoke.read_build_info(metadata, self.checksums)
        self.assertEqual(identity["commit"], "a" * 40)
        self.assertEqual(identity["signed"], "false")
        metadata.write_text(content + "commit=" + "b" * 40 + "\n")
        with self.assertRaisesRegex(smoke.SmokeError, "SHA256"):
            smoke.read_build_info(metadata, self.checksums)
        self.checksums.write_text(f"{smoke.sha256(metadata)}  ./{metadata.name}\n")
        with self.assertRaisesRegex(smoke.SmokeError, "duplicate"):
            smoke.read_build_info(metadata, self.checksums)

    def run_archive_with_identity(self, archive_name, platform, target):
        archive = self.make_archive("zip" if archive_name.endswith(".zip") else "tar")
        archive = archive.replace(self.root / archive_name)
        metadata = self.root / f"BUILD-INFO-{platform}.txt"
        metadata.write_text("tag=v1.2.3\ncommit=" + "a" * 40 +
                            "\nworkflow_run=https://github.com/example/repo/actions/runs/1\n"
                            f"target={target}\nsigned=false\nnotarized=n/a\n")
        self.checksums.write_text("".join(f"{smoke.sha256(p)}  {p.name}\n" for p in (archive, metadata)))
        report_path = self.root / "report.json"
        report_path.write_text('{"success":true,"build":{"target":"stale"}}')
        with mock.patch.object(smoke, "check_cli", return_value={"success": True, "body_cases": 35}) as check:
            result = smoke.main(["cli", "--archive", str(archive), "--checksums", str(self.checksums),
                                 "--build-info", str(metadata), "--output", str(report_path)])
        return result, json.loads(report_path.read_text()), check

    def test_archive_identity_matches_platform_and_target_before_execution(self):
        platforms = [
            ("postman-g-linux-x86_64.tar.gz", "linux", "x86_64-unknown-linux-gnu"),
            ("postman-g-windows-x86_64.zip", "windows", "x86_64-pc-windows-msvc"),
            ("postman-g-macos-universal.tar.gz", "macos", "universal-apple-darwin"),
        ]
        for archive, platform, target in platforms:
            with self.subTest(archive=archive, valid=True):
                result, report, check = self.run_archive_with_identity(archive, platform, target)
                self.assertEqual(result, 0)
                self.assertTrue(report["success"])
                self.assertEqual(report["build"]["target"], target)
                check.assert_called_once()
            for _, other_platform, other_target in platforms:
                if other_platform == platform:
                    continue
                for metadata_platform, metadata_target in ((platform, other_target), (other_platform, target),
                                                           (other_platform, other_target)):
                    with self.subTest(archive=archive, metadata=metadata_platform, target=metadata_target):
                        result, report, check = self.run_archive_with_identity(archive, metadata_platform, metadata_target)
                        self.assertNotEqual(result, 0)
                        self.assertFalse(report["success"])
                        self.assertIn("BUILD-INFO", report["error"])
                        self.assertNotIn("build", report)
                        check.assert_not_called()

    def test_custom_archive_cannot_claim_unverified_build_identity(self):
        result, report, check = self.run_archive_with_identity("custom.tar.gz", "linux", "x86_64-unknown-linux-gnu")
        self.assertNotEqual(result, 0)
        self.assertFalse(report["success"])
        self.assertNotIn("build", report)
        check.assert_not_called()

    def test_custom_archive_without_build_info_remains_usable(self):
        archive = self.make_archive()
        output = self.root / "report.json"
        with mock.patch.object(smoke, "check_cli", return_value={"success": True, "body_cases": 35}):
            self.assertEqual(smoke.main(["cli", "--archive", str(archive), "--checksums", str(self.checksums),
                                         "--output", str(output)]), 0)
        report = json.loads(output.read_text())
        self.assertTrue(report["success"])
        self.assertNotIn("build", report)


class RunnerTests(unittest.TestCase):
    def test_child_environment_removes_only_http_proxy_variables(self):
        settings = {key: "http://127.0.0.1:1" for key in
                    ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy")}
        settings.update({"POSTMAN_SMOKE_SENTINEL": "preserved", "FTP_PROXY": "preserved"})
        result = subprocess.CompletedProcess([], 0, '{"success":true}', "")
        with mock.patch.dict(os.environ, settings), smoke.running_server() as server:
            with mock.patch.object(smoke.subprocess, "run", return_value=result) as run:
                smoke.run_cli(Path("cli"), Path("fixture"), server)
            child = run.call_args.kwargs["env"]
            self.assertFalse(any(key.upper() in ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY") for key in child))
            self.assertEqual(child["POSTMAN_SMOKE_SENTINEL"], "preserved")
            self.assertEqual(child["FTP_PROXY"], "preserved")
            self.assertEqual(child.get("PATH"), os.environ.get("PATH"))
            self.assertEqual(os.environ["HTTP_PROXY"], settings["HTTP_PROXY"])

    def test_non_object_cli_json_replaces_old_success_from_main(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary, output = root / "cli", root / "report.json"
            binary.write_bytes(b"fixture")
            for stdout in ("[]", "null", "true", "42"):
                with self.subTest(stdout=stdout):
                    output.write_text('{"success":true}')
                    result = subprocess.CompletedProcess([], 0, stdout, "")
                    with mock.patch.object(smoke.subprocess, "run", return_value=result):
                        self.assertNotEqual(smoke.main(["cli", "--binary", str(binary), "--output", str(output)]), 0)
                    report = json.loads(output.read_text())
                    self.assertFalse(report["success"])
                    self.assertIn("JSON object", report["error"])

    def test_cleanup_failure_overrides_success_and_exits_nonzero(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary, output = root / "cli", root / "report.json"
            binary.write_bytes(b"fixture")
            output.write_text('{"success":true}')

            @contextlib.contextmanager
            def failed_cleanup(**_):
                yield temporary
                raise OSError("temporary cleanup failed")

            with mock.patch.object(smoke.tempfile, "TemporaryDirectory", failed_cleanup):
                with mock.patch.object(smoke, "check_cli", return_value={"success": True, "body_cases": 35}):
                    self.assertNotEqual(smoke.main(["cli", "--binary", str(binary), "--output", str(output)]), 0)
            report = json.loads(output.read_text())
            self.assertFalse(report["success"])
            self.assertEqual(report["error"], "temporary cleanup failed")

    def test_binary_mode_does_not_claim_build_identity(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary, output = root / "cli", root / "report.json"
            binary.write_bytes(b"fixture")
            with mock.patch.object(smoke, "check_cli", return_value={"success": True, "body_cases": 35}):
                self.assertEqual(smoke.main(["cli", "--binary", str(binary), "--output", str(output)]), 0)
            self.assertNotIn("build", json.loads(output.read_text()))

    def test_integrity_failure_replaces_a_previous_success_report(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            archive, checksums, output = root / "cli.tar.gz", root / "SHA256SUMS", root / "report.json"
            archive.write_bytes(b"untrusted archive")
            checksums.write_text("0" * 64 + "  cli.tar.gz\n")
            output.write_text('{"success": true}')
            self.assertEqual(smoke.main(["cli", "--archive", str(archive), "--checksums", str(checksums),
                                         "--output", str(output)]), 1)
            report = json.loads(output.read_text())
            self.assertFalse(report["success"])
            self.assertIn("SHA256", report["error"])

    def test_failed_cli_and_invalid_reports_are_not_success(self):
        with smoke.running_server() as server:
            for code, stdout in ((1, ""), (0, "not JSON"), (0, '{"success":false}')):
                with self.subTest(code=code, stdout=stdout):
                    result = subprocess.CompletedProcess([], code, stdout, "diagnostic")
                    with mock.patch.object(smoke.subprocess, "run", return_value=result):
                        with self.assertRaises(smoke.SmokeError):
                            smoke.run_cli(Path("cli"), Path("fixture.http.yml"), server)

    def test_successful_report_without_observed_bytes_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            with mock.patch.object(smoke, "run_cli", return_value={"success": True}):
                with self.assertRaisesRegex(smoke.SmokeError, "Body matrix"):
                    smoke.check_cli(Path("cli"), Path(temporary))

    def test_check_network_side_effect_is_rejected(self):
        def run(binary, fixture, server, check=False):
            server.capture("GET", "/unexpected", {}, b"")
            return {"success": True}
        with tempfile.TemporaryDirectory() as temporary:
            with mock.patch.object(smoke, "run_cli", side_effect=run):
                with self.assertRaisesRegex(smoke.SmokeError, "Check sent network"):
                    smoke.check_cli(Path("cli"), Path(temporary))

    def test_failure_report_is_written_without_claiming_native_acceptance(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary, output = root / "postman-g", root / "result.json"
            binary.write_bytes(b"fixture")
            with mock.patch.object(smoke, "check_cli", side_effect=smoke.SmokeError("wrong bytes")):
                self.assertEqual(smoke.main(["cli", "--binary", str(binary), "--output", str(output)]), 1)
            report = json.loads(output.read_text())
            self.assertFalse(report["success"])
            self.assertEqual(report["native_desktop_acceptance"], "not_run")
            self.assertEqual(report["binary"]["sha256"], smoke.sha256(binary))
            self.assertEqual(report["error"], "wrong bytes")


@unittest.skipUnless(os.environ.get("POSTMAN_SMOKE_TEST_CLI"), "set POSTMAN_SMOKE_TEST_CLI to run the built CLI")
class RealCliProxyTests(unittest.TestCase):
    def test_all_fixtures_bypass_an_enabled_proxy(self):
        binary = Path(os.environ["POSTMAN_SMOKE_TEST_CLI"]).resolve()
        with smoke.running_server() as proxy, tempfile.TemporaryDirectory() as temporary:
            # Empty bypass settings ensure this test fails if the child inherits proxies.
            settings = {name: proxy.url for name in
                        ("HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy")}
            settings.update({"NO_PROXY": "", "no_proxy": ""})
            output = Path(temporary) / "report.json"
            with mock.patch.dict(os.environ, settings):
                self.assertEqual(smoke.main(["cli", "--binary", str(binary), "--output", str(output)]), 0)
            report = json.loads(output.read_text())
            self.assertTrue(report["success"])
            self.assertEqual(report["body_cases"], 35)
            self.assertEqual(len(report["flows"]), 3)
            self.assertTrue(all(flow["check_no_requests"] for flow in report["flows"]))
            self.assertEqual(len(report["requests"]), 40)
            self.assertEqual(proxy.records, [])


@unittest.skipIf(os.name == "nt", "release publication runs in bash on Linux")
class PublicationTests(unittest.TestCase):
    def test_publish_requires_the_three_platform_smoke_reports(self):
        workflow = (smoke.ROOT / ".github/workflows/release.yml").read_text()
        step = workflow.split("      - name: Reject partial release output\n", 1)[1].split("      - name:", 1)[0]
        script = textwrap.dedent(step.split("        run: |\n", 1)[1])
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            dist = root / "dist"
            dist.mkdir()
            for name in ("app.dmg", "app.app.zip", "app.exe", "app.AppImage", "app.deb",
                         "postman-g-macos-universal.tar.gz", "postman-g-linux-x86_64.tar.gz",
                         "postman-g-windows-x86_64.zip", "BUILD-INFO-macos.txt",
                         "BUILD-INFO-linux.txt", "BUILD-INFO-windows.txt"):
                (dist / name).touch()
            for platforms, passed in (([], False), (["macos", "linux"], False),
                                      (["macos", "linux", "windows"], True),
                                      (["macos", "linux", "unexpected"], False),
                                      (["macos", "linux", "windows", "extra"], False)):
                with self.subTest(platforms=platforms):
                    for report in dist.glob("CLI-SMOKE-*.json"):
                        report.unlink()
                    for platform in platforms:
                        (dist / f"CLI-SMOKE-{platform}.json").write_text('{"success":true}')
                    result = subprocess.run(["bash", "-e", "-o", "pipefail", "-c", script], cwd=root,
                                            capture_output=True, text=True)
                    self.assertEqual(result.returncode == 0, passed, result.stderr)


if __name__ == "__main__":
    unittest.main()
