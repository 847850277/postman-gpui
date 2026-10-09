"""Check the resolved GUI stack and the workspace's headless dependency boundaries."""

from __future__ import annotations

import json
import subprocess
from pathlib import Path


def check(metadata: dict) -> list[str]:
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node["dependencies"] for node in metadata["resolve"]["nodes"]}
    errors = []

    for name, expected in (("gpui-kit", "0.7.1"), ("gpui-pre", "0.3.8"),
                           ("gpui-pre-platform", "0.3.8"), ("gpui-pre-macros", "0.3.8")):
        matches = [package for package in packages.values() if package["name"] == name]
        if len(matches) != 1 or matches[0]["version"] != expected:
            errors.append(f"Expected one {name} {expected}, found {[p['id'] for p in matches]}")
        elif matches[0].get("source") != "registry+https://github.com/rust-lang/crates.io-index":
            errors.append(f"{name} must use the pinned crates.io release")

    for package in packages.values():
        if package["name"] in {"gpui", "gpui_platform", "gpui_macros"}:
            errors.append(f"Unexpected second GPUI stack: {package['id']}")

    for member in metadata["workspace_members"]:
        name = packages[member]["name"]
        if name == "postman-gpui":
            continue
        pending, visited = [member], set()
        while pending:
            current = pending.pop()
            if current in visited:
                continue
            visited.add(current)
            dependency = packages[current]["name"]
            if dependency == "gpui" or dependency.startswith(("gpui-", "gpui_")):
                errors.append(f"{name} must remain headless, but depends on {packages[current]['id']}")
                break
            pending.extend(nodes.get(current, []))
    return errors


def main() -> None:
    result = subprocess.run(
        ["cargo", "metadata", "--locked", "--all-features", "--format-version", "1"],
        cwd=Path(__file__).resolve().parents[1], check=True, capture_output=True, text=True,
    )
    errors = check(json.loads(result.stdout))
    if errors:
        raise SystemExit("\n".join(errors))
    print("GPUI Kit 0.7.1 / GPUI 0.3.8: one stack; all engine, CLI, and MCP crates remain headless.")


if __name__ == "__main__":
    main()
