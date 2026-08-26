#!/usr/bin/env python3
"""Build, package, and validate Postman GPUI releases.

The script intentionally keeps the cargo-packager configuration in one place so
local builds and GitHub Actions produce the same application metadata.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
from functools import lru_cache
from pathlib import Path
from typing import Any, Iterable


ROOT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = ROOT / "Cargo.toml"
APP_BINARY = "postman-gpui"
PRODUCT_NAME = "Postman GPUI"
IDENTIFIER = "io.github.847850277.postman-gpui"
MACOS_MINIMUM_VERSION = "10.15.7"

ICON_PATHS = (
    ROOT / "assets/icons/32x32.png",
    ROOT / "assets/icons/128x128.png",
    ROOT / "assets/icons/128x128@2x.png",
    ROOT / "assets/icons/icon.png",
    ROOT / "assets/icons/icon.icns",
    ROOT / "assets/icons/icon.ico",
)

FONT_ASSETS = {
    ROOT / "assets/fonts/space-grotesk/SpaceGrotesk[wght].ttf": (
        "acad6de1fc93436f5c0f1f4137751ef04f1aea3063e7036535970ffcfbd79f72"
    ),
    ROOT / "assets/fonts/manrope/Manrope[wght].ttf": (
        "d0639be45d0af36e798172419d7bd173c4bd4f29e2b76cbb69db1d11bf8b0a40"
    ),
    ROOT / "assets/fonts/jetbrains-mono/JetBrainsMono[wght].ttf": (
        "48715a42ec242c21e9f02692891e147d022299a52e48d5e413e1a942193ffeda"
    ),
}

FONT_LICENSES = (
    ROOT / "assets/fonts/space-grotesk/OFL.txt",
    ROOT / "assets/fonts/manrope/OFL.txt",
    ROOT / "assets/fonts/jetbrains-mono/OFL.txt",
    ROOT / "assets/fonts/README.md",
)

FONT_LICENSE_TARGETS = {
    ROOT / "assets/fonts/space-grotesk/OFL.txt": "licenses/space-grotesk-OFL.txt",
    ROOT / "assets/fonts/manrope/OFL.txt": "licenses/manrope-OFL.txt",
    ROOT / "assets/fonts/jetbrains-mono/OFL.txt": "licenses/jetbrains-mono-OFL.txt",
    ROOT / "assets/fonts/README.md": "licenses/bundled-fonts.md",
}

PLATFORM_FORMATS = {
    "macos": ("app", "dmg"),
    "windows": ("nsis",),
    "linux": ("appimage", "deb"),
}

DEFAULT_TARGETS = {
    "macos": "aarch64-apple-darwin",
    "windows": "x86_64-pc-windows-msvc",
    "linux": "x86_64-unknown-linux-gnu",
}

TAG_PATTERN = re.compile(
    r"^v(?P<base>[0-9]+\.[0-9]+\.[0-9]+)"
    r"(?P<prerelease>-[0-9A-Za-z][0-9A-Za-z.-]*)?$"
)


class ReleaseError(RuntimeError):
    """Raised for an actionable release configuration error."""


@lru_cache(maxsize=1)
def load_manifest() -> dict[str, Any]:
    """Read package metadata through Cargo so Python 3.9 remains supported."""

    result = subprocess.run(
        [
            "cargo",
            "metadata",
            "--no-deps",
            "--format-version",
            "1",
            "--manifest-path",
            str(MANIFEST_PATH),
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    metadata = json.loads(result.stdout)
    manifest = str(MANIFEST_PATH.resolve())
    for package in metadata.get("packages", []):
        if str(Path(package["manifest_path"]).resolve()) == manifest:
            return package
    raise ReleaseError("cargo metadata did not return the root package")


def normalize_platform(value: str | None) -> str:
    raw = (value or platform.system()).strip().lower()
    aliases = {
        "darwin": "macos",
        "mac": "macos",
        "macos": "macos",
        "windows": "windows",
        "win32": "windows",
        "linux": "linux",
    }
    try:
        return aliases[raw]
    except KeyError as error:
        supported = ", ".join(sorted(PLATFORM_FORMATS))
        raise ReleaseError(f"unsupported platform {raw!r}; expected one of {supported}") from error


def rust_host_target() -> str:
    result = subprocess.run(
        ["rustc", "-vV"],
        check=True,
        capture_output=True,
        text=True,
    )
    for line in result.stdout.splitlines():
        if line.startswith("host: "):
            return line.removeprefix("host: ").strip()
    raise ReleaseError("rustc -vV did not report a host target")


def resolve_target(platform_name: str, requested: str | None) -> str:
    if requested:
        return requested
    current_platform = normalize_platform(None)
    if current_platform == platform_name:
        return rust_host_target()
    return DEFAULT_TARGETS[platform_name]


def release_binary_directory(target: str) -> Path:
    return ROOT / "target" / target / "release"


def _required_env_pair(first: str, second: str) -> None:
    has_first = bool(os.environ.get(first))
    has_second = bool(os.environ.get(second))
    if has_first != has_second:
        raise ReleaseError(f"{first} and {second} must either both be set or both be unset")


def validate_signing_environment(platform_name: str) -> None:
    if platform_name == "macos":
        _required_env_pair("APPLE_CERTIFICATE", "APPLE_CERTIFICATE_PASSWORD")
        if os.environ.get("APPLE_CERTIFICATE") and not os.environ.get(
            "APPLE_SIGNING_IDENTITY"
        ):
            raise ReleaseError(
                "APPLE_SIGNING_IDENTITY is required when APPLE_CERTIFICATE is provided"
            )
        apple_id_values = [
            os.environ.get("APPLE_ID"),
            os.environ.get("APPLE_PASSWORD"),
            os.environ.get("APPLE_TEAM_ID"),
        ]
        if any(apple_id_values) and not all(apple_id_values):
            raise ReleaseError(
                "APPLE_ID, APPLE_PASSWORD, and APPLE_TEAM_ID must be provided together"
            )
    elif platform_name == "windows":
        _required_env_pair("WINDOWS_CERTIFICATE", "WINDOWS_CERTIFICATE_PASSWORD")
        certificate_values = [
            os.environ.get("WINDOWS_CERTIFICATE"),
            os.environ.get("WINDOWS_CERTIFICATE_PASSWORD"),
            os.environ.get("WINDOWS_CERTIFICATE_THUMBPRINT"),
        ]
        if any(certificate_values) and not all(certificate_values):
            raise ReleaseError(
                "WINDOWS_CERTIFICATE, WINDOWS_CERTIFICATE_PASSWORD, and "
                "WINDOWS_CERTIFICATE_THUMBPRINT must be provided together"
            )


def packager_config(
    platform_name: str,
    target: str,
    binaries_dir: Path,
    out_dir: Path,
) -> dict[str, Any]:
    """Return a cargo-packager 0.11 configuration for one native platform."""

    platform_name = normalize_platform(platform_name)
    validate_signing_environment(platform_name)
    package = load_manifest()
    version = str(package["version"])
    repository = str(package["repository"])
    authors = [str(author) for author in package.get("authors", [])]

    config: dict[str, Any] = {
        "name": APP_BINARY,
        "productName": PRODUCT_NAME,
        "version": version,
        "identifier": IDENTIFIER,
        "binaries": [{"path": APP_BINARY, "main": True}],
        "binariesDir": str(binaries_dir.resolve()),
        "outDir": str(out_dir.resolve()),
        "targetTriple": target,
        "description": str(package["description"]),
        "longDescription": (
            "A native HTTP client for constructing, sending, inspecting, and replaying "
            "API requests with persistent local history."
        ),
        "homepage": repository,
        "authors": authors,
        "publisher": "847850277",
        "licenseFile": str((ROOT / "LICENSE").resolve()),
        "copyright": "Copyright © 2025-2026 847850277",
        "category": "DeveloperTool",
        "icons": [str(icon.resolve()) for icon in ICON_PATHS],
        "resources": [
            {"src": str(source.resolve()), "target": target}
            for source, target in FONT_LICENSE_TARGETS.items()
        ],
    }

    if platform_name == "macos":
        macos: dict[str, Any] = {"minimumSystemVersion": MACOS_MINIMUM_VERSION}
        signing_identity = os.environ.get("APPLE_SIGNING_IDENTITY")
        if signing_identity:
            macos["signingIdentity"] = signing_identity
        config["macos"] = macos
    elif platform_name == "windows":
        windows: dict[str, Any] = {
            "digestAlgorithm": "sha256",
            "timestampUrl": "http://timestamp.digicert.com",
            "tsp": True,
        }
        certificate_thumbprint = os.environ.get("WINDOWS_CERTIFICATE_THUMBPRINT")
        if certificate_thumbprint:
            windows["certificateThumbprint"] = certificate_thumbprint
        config["windows"] = windows
        config["nsis"] = {
            "installerIcon": str((ROOT / "assets/icons/icon.ico").resolve()),
            "installMode": "currentUser",
        }
    else:
        config["linux"] = {"generateDesktopEntry": True}
        config["deb"] = {
            "packageName": APP_BINARY,
            "section": "devel",
            "priority": "optional",
            "depends": [
                "libasound2",
                "libfontconfig1",
                "libvulkan1",
                "libwayland-client0",
                "libxcb1",
                "libxkbcommon0",
                "libxkbcommon-x11-0",
            ],
        }
        # GPUI loads the Vulkan and window-system entry points dynamically, so
        # linuxdeploy cannot discover all of them through ldd alone.
        config["appimage"] = {
            "libs": [
                "libvulkan.so.1",
                "libwayland-client.so.0",
                "libxcb.so.1",
                "libxkbcommon.so.0",
                "libxkbcommon-x11.so.0",
            ]
        }

    return config


def parse_formats(platform_name: str, value: str | None) -> tuple[str, ...]:
    formats = tuple(
        item.strip().lower()
        for item in (value.split(",") if value else PLATFORM_FORMATS[platform_name])
        if item.strip()
    )
    if not formats:
        raise ReleaseError("at least one package format is required")
    invalid = sorted(set(formats).difference(PLATFORM_FORMATS[platform_name]))
    if invalid:
        raise ReleaseError(
            f"formats {', '.join(invalid)} are not supported for {platform_name}; "
            f"expected {', '.join(PLATFORM_FORMATS[platform_name])}"
        )
    return formats


def run(command: Iterable[str], *, environment: dict[str, str] | None = None) -> None:
    command = list(command)
    rendered = " ".join(command)
    print(f"+ {rendered}", flush=True)
    subprocess.run(command, cwd=ROOT, env=environment, check=True)


def build_release(platform_name: str, target: str, universal_macos: bool) -> Path:
    environment = os.environ.copy()
    if platform_name == "macos":
        environment.setdefault("MACOSX_DEPLOYMENT_TARGET", MACOS_MINIMUM_VERSION)

    if universal_macos:
        if platform_name != "macos":
            raise ReleaseError("--universal-macos can only be used on macOS")
        if normalize_platform(None) != "macos":
            raise ReleaseError("universal macOS binaries must be built on a macOS host")

        targets = ("aarch64-apple-darwin", "x86_64-apple-darwin")
        run(["rustup", "target", "add", *targets], environment=environment)
        for architecture_target in targets:
            run(
                [
                    "cargo",
                    "build",
                    "--locked",
                    "--release",
                    "--target",
                    architecture_target,
                ],
                environment=environment,
            )

        output_directory = release_binary_directory("universal-apple-darwin")
        output_directory.mkdir(parents=True, exist_ok=True)
        run(
            [
                "lipo",
                "-create",
                str(release_binary_directory(targets[0]) / APP_BINARY),
                str(release_binary_directory(targets[1]) / APP_BINARY),
                "-output",
                str(output_directory / APP_BINARY),
            ],
            environment=environment,
        )
        return output_directory

    run(
        ["cargo", "build", "--locked", "--release", "--target", target],
        environment=environment,
    )
    return release_binary_directory(target)


def archive_macos_app(out_dir: Path, version: str, target: str) -> None:
    architecture = target.split("-")[0]
    for app_bundle in sorted(out_dir.glob("*.app")):
        archive = out_dir / f"Postman-GPUI_{version}_{architecture}.app.zip"
        if archive.exists():
            archive.unlink()
        run(
            [
                "ditto",
                "-c",
                "-k",
                "--sequesterRsrc",
                "--keepParent",
                str(app_bundle),
                str(archive),
            ]
        )


def package_release(args: argparse.Namespace) -> None:
    platform_name = normalize_platform(args.platform)
    universal_macos = bool(args.universal_macos)
    if universal_macos and platform_name != "macos":
        raise ReleaseError("--universal-macos can only be used with --platform macos")
    if universal_macos and args.target:
        raise ReleaseError("--target cannot be combined with --universal-macos")
    if universal_macos and normalize_platform(None) != "macos":
        raise ReleaseError("universal macOS packages must be created on a macOS host")

    target = "universal-apple-darwin" if universal_macos else resolve_target(
        platform_name, args.target
    )
    formats = parse_formats(platform_name, args.formats)
    out_dir = Path(args.out_dir).resolve()

    if args.skip_build:
        binaries_dir = release_binary_directory(target)
    else:
        binaries_dir = build_release(platform_name, target, universal_macos)

    executable = binaries_dir / (
        f"{APP_BINARY}.exe" if platform_name == "windows" else APP_BINARY
    )
    if not executable.is_file():
        raise ReleaseError(f"release binary does not exist: {executable}")

    if shutil.which("cargo-packager") is None:
        raise ReleaseError(
            "cargo-packager is not installed; run "
            "`cargo install cargo-packager --version 0.11.8 --locked`"
        )

    out_dir.mkdir(parents=True, exist_ok=True)
    config = packager_config(platform_name, target, binaries_dir, out_dir)
    config_path: Path | None = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w",
            encoding="utf-8",
            suffix=".json",
            prefix="postman-gpui-packager-",
            dir=ROOT,
            delete=False,
        ) as config_file:
            json.dump(config, config_file, indent=2)
            config_file.write("\n")
            config_path = Path(config_file.name)

        run(
            [
                "cargo",
                "packager",
                "--config",
                str(config_path),
                "--formats",
                ",".join(formats),
            ]
        )
    finally:
        if config_path is not None:
            config_path.unlink(missing_ok=True)

    if platform_name == "macos" and "app" in formats:
        archive_macos_app(out_dir, str(load_manifest()["version"]), target)


def parse_release_tag(tag: str, manifest_version: str) -> tuple[bool, str]:
    match = TAG_PATTERN.fullmatch(tag)
    if match is None:
        raise ReleaseError(
            f"invalid release tag {tag!r}; expected vMAJOR.MINOR.PATCH or a prerelease suffix"
        )
    if match.group("base") != manifest_version:
        raise ReleaseError(
            f"tag {tag!r} does not match Cargo.toml version {manifest_version}"
        )
    return bool(match.group("prerelease")), match.group("base")


def verify_release(tag: str) -> dict[str, str]:
    package = load_manifest()
    manifest_version = str(package["version"])
    prerelease, version = parse_release_tag(tag, manifest_version)

    required_files = [
        *ICON_PATHS,
        *FONT_ASSETS,
        *FONT_LICENSES,
        ROOT / "assets/icons/icon.svg",
        ROOT / "assets/icons/windows/icon.rc",
        ROOT / "examples/verify_runtime_assets.rs",
        ROOT / ".github/workflows/release.yml",
        ROOT / "CHANGELOG.md",
        ROOT / "docs/installation.md",
        ROOT / "docs/autofill-contract.md",
        ROOT / "docs/releasing.md",
        ROOT / "docs/release-smoke-test.md",
    ]
    missing = [str(path.relative_to(ROOT)) for path in required_files if not path.is_file()]
    if missing:
        raise ReleaseError(f"required release files are missing: {', '.join(missing)}")

    for font_path, expected_digest in FONT_ASSETS.items():
        actual_digest = hashlib.sha256(font_path.read_bytes()).hexdigest()
        if actual_digest != expected_digest:
            raise ReleaseError(
                f"bundled font checksum mismatch for {font_path.relative_to(ROOT)}"
            )

    readme = (ROOT / "README.md").read_text(encoding="utf-8")
    if "github.com/yourusername" in readme:
        raise ReleaseError("README.md still contains the placeholder clone URL")
    if str(package["repository"]) not in readme:
        raise ReleaseError("README.md does not link to the canonical repository")

    changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    if f"## [{version}]" not in changelog:
        raise ReleaseError(f"CHANGELOG.md does not contain a [{version}] release section")

    for platform_name, target in DEFAULT_TARGETS.items():
        config = packager_config(
            platform_name,
            target,
            release_binary_directory(target),
            ROOT / "dist",
        )
        json.dumps(config)

    return {
        "tag": tag,
        "version": version,
        "prerelease": str(prerelease).lower(),
    }


def write_github_outputs(values: dict[str, str]) -> None:
    output_path = os.environ.get("GITHUB_OUTPUT")
    if not output_path:
        raise ReleaseError("--github-output requires the GITHUB_OUTPUT environment variable")
    with Path(output_path).open("a", encoding="utf-8") as output_file:
        for key, value in values.items():
            output_file.write(f"{key}={value}\n")


def create_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    config_parser = subparsers.add_parser("config", help="print resolved packager JSON")
    config_parser.add_argument("--platform", choices=sorted(PLATFORM_FORMATS))
    config_parser.add_argument("--target")
    config_parser.add_argument("--binaries-dir")
    config_parser.add_argument("--out-dir", default=str(ROOT / "dist"))

    package_parser = subparsers.add_parser("package", help="build and package the application")
    package_parser.add_argument("--platform", choices=sorted(PLATFORM_FORMATS))
    package_parser.add_argument("--target")
    package_parser.add_argument("--formats", help="comma-separated native package formats")
    package_parser.add_argument("--out-dir", default=str(ROOT / "dist"))
    package_parser.add_argument("--skip-build", action="store_true")
    package_parser.add_argument("--universal-macos", action="store_true")

    verify_parser = subparsers.add_parser("verify", help="validate release metadata and tag")
    verify_parser.add_argument("--tag", required=True)
    verify_parser.add_argument("--github-output", action="store_true")

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = create_parser()
    args = parser.parse_args(argv)
    try:
        if args.command == "config":
            platform_name = normalize_platform(args.platform)
            target = resolve_target(platform_name, args.target)
            binaries_dir = Path(args.binaries_dir) if args.binaries_dir else release_binary_directory(
                target
            )
            config = packager_config(
                platform_name,
                target,
                binaries_dir,
                Path(args.out_dir),
            )
            print(json.dumps(config, indent=2))
        elif args.command == "package":
            package_release(args)
        else:
            values = verify_release(args.tag)
            if args.github_output:
                write_github_outputs(values)
            print(
                f"release verification passed: {values['tag']} "
                f"(prerelease={values['prerelease']})"
            )
        return 0
    except (ReleaseError, subprocess.CalledProcessError, OSError) as error:
        print(f"release error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
