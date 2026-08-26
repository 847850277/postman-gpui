from __future__ import annotations

import argparse
import importlib.util
import tempfile
import unittest
from pathlib import Path


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
                    self.assertEqual(len(config["resources"]), 4)
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


if __name__ == "__main__":
    unittest.main()
