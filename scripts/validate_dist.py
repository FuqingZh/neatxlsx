#!/usr/bin/env python3
"""Validate neatxlsx wheel identity and required package contents."""

from __future__ import annotations

import argparse
import re
import zipfile
from pathlib import Path

WHEEL = re.compile(r"^neatxlsx-(?P<version>[^-]+)-.+\.whl$")
REQUIRED = {
    "neatxlsx/__init__.py",
    "neatxlsx/errors.py",
    "neatxlsx/spec.py",
    "neatxlsx/writer.py",
}


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dist-dir", type=Path, default=Path("dist"))
    parser.add_argument("--expected-version")
    return parser.parse_args()


def validate(dist_dir: Path, expected_version: str | None = None) -> str:
    wheels = sorted(dist_dir.glob("neatxlsx-*.whl"))
    if not wheels:
        raise RuntimeError(f"No neatxlsx wheel found in {dist_dir.resolve()}")
    versions: set[str] = set()
    for wheel in wheels:
        match = WHEEL.match(wheel.name)
        if match is None:
            raise RuntimeError(f"Unexpected wheel name: {wheel.name}")
        versions.add(match.group("version"))
        if "-abi3-" not in wheel.name:
            raise RuntimeError(f"Wheel isn't abi3: {wheel.name}")
        with zipfile.ZipFile(wheel) as archive:
            names = set(archive.namelist())
        missing = REQUIRED - names
        if missing:
            raise RuntimeError(f"{wheel.name} missing package files: {sorted(missing)}")
        if not any(
            name.startswith("neatxlsx/_native")
            and name.endswith((".so", ".pyd", ".dylib"))
            for name in names
        ):
            raise RuntimeError(f"{wheel.name} missing neatxlsx._native")
    if len(versions) != 1:
        raise RuntimeError(f"Mixed wheel versions: {sorted(versions)}")
    version = next(iter(versions))
    if expected_version is not None and version != expected_version:
        raise RuntimeError(f"Expected version {expected_version!r}, found {version!r}")
    return version


def main() -> None:
    args = _parse_args()
    version = validate(args.dist_dir, args.expected_version)
    print(f"Validated neatxlsx distribution version {version}")


if __name__ == "__main__":
    main()
