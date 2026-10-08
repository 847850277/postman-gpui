from __future__ import annotations

import argparse
import hashlib
import importlib.util
import io
import os
import shlex
import shutil
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock


SCRIPT_PATH = Path(__file__).resolve().parents[1] / "release.py"
SPEC = importlib.util.spec_from_file_location("postman_gpui_release", SCRIPT_PATH)
assert SPEC is not None and SPEC.loader is not None
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)


class ReleaseConfigurationTests(unittest.TestCase):
    def test_release_and_prerelease_tags_match_manifest_version(self) -> None:
        self.assertEqual(release.parse_release_tag("v0.1.0", "0.1.0"), (False, "0.1.0"))
        self.assertEqual(
            release.parse_release_tag("v0.1.0-rc.1", "0.1.0"),
            (True, "0.1.0"),
        )

    def test_mismatched_or_unsafe_tags_are_rejected(self) -> None:
        for tag in ("0.1.0", "v0.2.0", "v0.1.0;echo-bad"):
            with self.subTest(tag=tag), self.assertRaises(release.ReleaseError):
                release.parse_release_tag(tag, "0.1.0")

    def test_each_platform_has_native_configuration(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cases = {
                "macos": ("aarch64-apple-darwin", "macos"),
                "windows": ("x86_64-pc-windows-msvc", "nsis"),
                "linux": ("x86_64-unknown-linux-gnu", "deb"),
            }
            for platform_name, (target, expected_section) in cases.items():
                with self.subTest(platform=platform_name):
                    config = release.packager_config(
                        platform_name,
                        target,
                        root / "bin",
                        root / "dist",
                    )
                    self.assertEqual(config["version"], "0.1.0")
                    self.assertEqual(config["identifier"], release.IDENTIFIER)
                    self.assertEqual(config["targetTriple"], target)
                    self.assertIn(expected_section, config)
                    self.assertTrue(all(Path(icon).is_file() for icon in config["icons"]))
                    self.assertEqual(len(config["resources"]), 3)
                    self.assertTrue(
                        all(
                            Path(resource["src"]).is_file()
                            and resource["target"].startswith("licenses/")
                            for resource in config["resources"]
                        )
                    )
                    if platform_name == "windows":
                        self.assertTrue(config["windows"]["tsp"])

    def test_formats_cannot_cross_platform_boundaries(self) -> None:
        self.assertEqual(release.parse_formats("linux", "appimage,deb"), ("appimage", "deb"))
        with self.assertRaises(release.ReleaseError):
            release.parse_formats("windows", "dmg")

    def test_universal_macos_options_are_unambiguous(self) -> None:
        with self.assertRaisesRegex(release.ReleaseError, "--platform macos"):
            release.package_release(
                argparse.Namespace(platform="linux", universal_macos=True, target=None)
            )
        with self.assertRaisesRegex(release.ReleaseError, "--target"):
            release.package_release(
                argparse.Namespace(
                    platform="macos",
                    universal_macos=True,
                    target="aarch64-apple-darwin",
                )
            )

    def test_release_assets_and_documentation_are_complete(self) -> None:
        self.assertEqual(
            release.verify_release("v0.1.0-rc.1"),
            {"tag": "v0.1.0-rc.1", "version": "0.1.0", "prerelease": "true"},
        )

    def test_linux_smoke_install_uses_an_absolute_deb_path(self) -> None:
        workflow = (release.ROOT / ".github/workflows/release.yml").read_text(
            encoding="utf-8"
        )
        self.assertIn('deb=$(realpath "$deb")', workflow)
        self.assertIn('sudo apt-get install --yes "$deb"', workflow)


@unittest.skipIf(os.name == "nt", "AppImage permissions require POSIX")
class AppImagePackagingTests(unittest.TestCase):
    def setUp(self) -> None:
        temporary = tempfile.TemporaryDirectory(prefix="postman appimage tests ")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.cache = self.root / "cache"
        self.target = "x86_64-unknown-linux-gnu"
        self.launcher = self.cache / ".cargo-packager/AppImage/AppRun-x86_64"
        self.payload = b"verified launcher fixture"
        patches = [
            mock.patch.dict(os.environ, {"XDG_CACHE_HOME": str(self.cache)}),
            mock.patch.dict(
                release.APPIMAGE_LAUNCHER_SHA256,
                {"x86_64": hashlib.sha256(self.payload).hexdigest()},
            ),
        ]
        for patch in patches:
            patch.start()
            self.addCleanup(patch.stop)

    def test_fresh_launcher_and_preexisting_owner_only_cache_are_executable_by_all(self) -> None:
        with mock.patch.object(release, "urlopen", return_value=io.BytesIO(self.payload)) as download:
            release.prepare_appimage_launcher(self.target)
        download.assert_called_once()
        self.assertEqual(self.launcher.read_bytes(), self.payload)
        self.assertEqual(stat.S_IMODE(self.launcher.stat().st_mode), 0o755)

        for previous_mode in (0o764, 0o744, 0o700):
            with self.subTest(mode=oct(previous_mode)):
                self.launcher.chmod(previous_mode)
                with mock.patch.object(release, "urlopen") as download:
                    release.prepare_appimage_launcher(self.target)
                download.assert_not_called()
                self.assertEqual(stat.S_IMODE(self.launcher.stat().st_mode), 0o755)
                self.assertEqual(self.launcher.read_bytes(), self.payload)

    def test_unverified_download_is_not_installed(self) -> None:
        with mock.patch.object(release, "urlopen", return_value=io.BytesIO(b"wrong download")):
            with self.assertRaisesRegex(release.ReleaseError, "checksum mismatch"):
                release.prepare_appimage_launcher(self.target)
        self.assertFalse(self.launcher.exists())

    def test_unverified_cached_launcher_is_not_made_executable(self) -> None:
        self.launcher.parent.mkdir(parents=True)
        self.launcher.write_bytes(b"wrong cache")
        self.launcher.chmod(0o644)
        with mock.patch.object(release, "urlopen") as download:
            with self.assertRaisesRegex(release.ReleaseError, "checksum mismatch"):
                release.prepare_appimage_launcher(self.target)
        download.assert_not_called()
        self.assertEqual(stat.S_IMODE(self.launcher.stat().st_mode), 0o644)

    def test_relative_xdg_cache_uses_the_same_fallback_as_packager(self) -> None:
        with mock.patch.dict(os.environ, {"XDG_CACHE_HOME": "relative-cache"}):
            with mock.patch.object(release.Path, "home", return_value=self.root):
                with mock.patch.object(release, "urlopen", return_value=io.BytesIO(self.payload)):
                    release.prepare_appimage_launcher(self.target)
        launcher = self.root / ".cache/.cargo-packager/AppImage/AppRun-x86_64"
        self.assertEqual(stat.S_IMODE(launcher.stat().st_mode), 0o755)
        self.assertFalse(self.launcher.exists())

    def make_appdir(self) -> Path:
        appdir = self.root / "application.AppDir"
        (appdir / "usr/bin").mkdir(parents=True, exist_ok=True)
        for relative in (".", "usr", "usr/bin"):
            (appdir / relative).chmod(0o755)
        for relative in ("AppRun", "usr/bin/postman-gpui"):
            executable = appdir / relative
            executable.write_bytes(self.payload)
            executable.chmod(0o755)
        return appdir

    def test_appdir_check_rejects_owner_only_launchers_and_main_binaries(self) -> None:
        for relative in ("AppRun", "usr/bin/postman-gpui"):
            for mode in (0o744, 0o764, 0o700, 0o644):
                with self.subTest(path=relative, mode=oct(mode)):
                    appdir = self.make_appdir()
                    (appdir / relative).chmod(mode)
                    with self.assertRaisesRegex(release.ReleaseError, "expected 0755"):
                        release.verify_appdir_permissions(appdir)
        release.verify_appdir_permissions(self.make_appdir())

    def test_appdir_check_rejects_missing_executable_and_private_parent_directory(self) -> None:
        appdir = self.make_appdir()
        (appdir / "AppRun").unlink()
        with self.assertRaisesRegex(release.ReleaseError, "executable is missing"):
            release.verify_appdir_permissions(appdir)
        appdir = self.make_appdir()
        (appdir / "usr").chmod(0o700)
        with self.assertRaisesRegex(release.ReleaseError, "readable/searchable by all"):
            release.verify_appdir_permissions(appdir)

    def test_packaging_copy_keeps_public_execute_bits_despite_private_parent_umask(self) -> None:
        with mock.patch.object(release, "urlopen", return_value=io.BytesIO(self.payload)):
            release.prepare_appimage_launcher(self.target)
        copied = self.root / "AppRun"
        previous_umask = os.umask(0o077)
        try:
            release.run(["cp", str(self.launcher), str(copied)], umask=0o022)
            observed_umask = os.umask(0o077)
        finally:
            os.umask(previous_umask)
        self.assertEqual(observed_umask, 0o077)
        self.assertEqual(stat.S_IMODE(copied.stat().st_mode), 0o755)

    def make_appimage(self, modes: dict[str, int] | None = None) -> Path:
        if not shutil.which("mksquashfs") or not shutil.which("unsquashfs"):
            self.skipTest("install squashfs-tools (Linux) or squashfs (Homebrew)")
        appdir = self.make_appdir()
        for relative, mode in (modes or {}).items():
            (appdir / relative).chmod(mode)
        squashfs = self.root / "payload.squashfs"
        subprocess.run(
            ["mksquashfs", str(appdir), str(squashfs), "-noappend", "-no-progress",
             "-no-xattrs", "-processors", "1"],
            check=True, stdout=subprocess.DEVNULL,
        )
        # A real SquashFS with a portable shell header standing in for the Linux
        # runtime. Its extraction deliberately reproduces type2-runtime's 0700
        # directories, while --appimage-offset points at the original metadata.
        header = (
            '#!/bin/sh\nset -eu\ncase "$1" in\n'
            '  --appimage-offset) echo 4096 ;;\n'
            '  --appimage-extract)\n'
            f'    {shlex.quote(shutil.which("unsquashfs"))} -no-progress -no-xattrs '
            '-o 4096 -d squashfs-root "$0" >/dev/null\n'
            '    find squashfs-root -type d -exec chmod 700 {} \\;\n'
            '    ;;\n'
            '  *) exit 2 ;;\nesac\nexit 0\n'
        ).encode()
        self.assertLess(len(header), 4096)
        appimage = self.root / "fixture.AppImage"
        appimage.write_bytes(header.ljust(4096, b"\0") + squashfs.read_bytes())
        appimage.chmod(0o755)
        return appimage

    def test_packaged_permissions_are_independent_of_runtime_extraction_modes(self) -> None:
        appimage = self.make_appimage()
        subprocess.run([str(appimage), "--appimage-extract"], cwd=self.root, check=True)
        for relative in (".", "usr", "usr/bin"):
            directory = self.root / "squashfs-root" / relative
            self.assertEqual(stat.S_IMODE(directory.stat().st_mode), 0o700)
        release.verify_appimage(appimage)

    def test_final_artifact_check_rejects_bad_packaged_permissions(self) -> None:
        cases = [("AppRun", 0o744), ("usr/bin/postman-gpui", 0o744),
                 (".", 0o700), ("usr", 0o700), ("usr/bin", 0o700)]
        for relative, mode in cases:
            with self.subTest(path=relative, mode=oct(mode)):
                appimage = self.make_appimage({relative: mode})
                with self.assertRaisesRegex(release.ReleaseError, f"mode {mode:04o}"):
                    release.verify_appimage(appimage)

    def test_verified_extraction_retains_packaged_permissions_for_smoke_test(self) -> None:
        appimage = self.make_appimage()
        destination = self.root / "retained AppDir"
        previous_umask = os.umask(0o077)
        try:
            release.verify_appimage(appimage, extract_to=destination)
        finally:
            os.umask(previous_umask)
        release.verify_appdir_permissions(destination)
        self.assertEqual((destination / "AppRun").read_bytes(), self.payload)

    def test_missing_unsquashfs_has_actionable_error(self) -> None:
        with mock.patch.object(release.shutil, "which", return_value=None):
            with self.assertRaisesRegex(release.ReleaseError, "squashfs-tools"):
                release.verify_appimage(self.root / "fixture.AppImage")


if __name__ == "__main__":
    unittest.main()
