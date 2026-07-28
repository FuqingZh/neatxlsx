#!/usr/bin/env python3
"""Select deterministic neatxlsx validation lanes from changed paths."""

from __future__ import annotations

import argparse
import json
import subprocess
from dataclasses import dataclass


@dataclass(frozen=True)
class Lane:
    name: str
    command: str
    reasons: tuple[str, ...]


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base")
    parser.add_argument("--head", default="HEAD")
    parser.add_argument("--json", action="store_true")
    return parser.parse_args()


def _changed_paths(base: str | None, head: str) -> tuple[str, ...]:
    if base is None:
        result = subprocess.run(
            ["git", "status", "--short"],
            check=True,
            capture_output=True,
            text=True,
        )
        return tuple(line[3:] for line in result.stdout.splitlines() if len(line) > 3)
    result = subprocess.run(
        ["git", "diff", "--name-only", base, head],
        check=True,
        capture_output=True,
        text=True,
    )
    return tuple(line for line in result.stdout.splitlines() if line)


def select_lanes(paths: tuple[str, ...]) -> tuple[Lane, ...]:
    if not paths:
        return ()
    lanes: list[Lane] = []
    rust = tuple(
        path
        for path in paths
        if path.startswith("crates/")
        or path in {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml"}
    )
    python = tuple(
        path
        for path in paths
        if path.startswith(("python/", "tests/", "scripts/"))
        or path in {"pyproject.toml", "pdm.lock"}
    )
    package = tuple(
        path
        for path in paths
        if path.startswith((".github/", "python/", "crates/"))
        or path
        in {
            "Cargo.toml",
            "Cargo.lock",
            "pyproject.toml",
            "pdm.lock",
            "LICENSE",
        }
    )
    if rust:
        lanes.append(Lane("rust", "pdm run rust-test", rust))
    if python:
        lanes.append(Lane("python", "pdm run test", python))
    if package:
        lanes.append(Lane("package", "pdm run package", package))
    if not lanes:
        lanes.append(Lane("static", "pdm run lint", paths))
    return tuple(lanes)


def main() -> None:
    args = _parse_args()
    lanes = select_lanes(_changed_paths(args.base, args.head))
    payload = [
        {"lane": lane.name, "command": lane.command, "reasons": list(lane.reasons)}
        for lane in lanes
    ]
    if args.json:
        print(json.dumps(payload, indent=2, sort_keys=True))
        return
    if not lanes:
        print("No changed paths; no affected lane.")
        return
    for lane in lanes:
        print(f"{lane.name}: {lane.command}")
        for reason in lane.reasons:
            print(f"  - {reason}")


if __name__ == "__main__":
    main()
