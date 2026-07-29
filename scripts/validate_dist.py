#!/usr/bin/env python3
"""Validate the identity and contents of neatxlsx release distributions."""

from __future__ import annotations

import argparse
import re
import tarfile
import zipfile
from pathlib import Path

WHEEL = re.compile(r"^neatxlsx-(?P<version>[^-]+)-.+\.whl$")
SDIST = re.compile(r"^neatxlsx-(?P<version>[^-]+)\.tar\.gz$")
REQUIRED_WHEEL_FILES = {
    "neatxlsx/__init__.py",
    "neatxlsx/errors.py",
    "neatxlsx/spec.py",
    "neatxlsx/writer.py",
}
REQUIRED_SDIST_FILES = {
    "Cargo.toml",
    "crates/neatxlsx_core/Cargo.toml",
    "crates/neatxlsx_py/Cargo.toml",
    "pyproject.toml",
    "python/neatxlsx/__init__.py",
}


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dist-dir", type=Path, default=Path("dist"))
    parser.add_argument("--expected-version")
    parser.add_argument("--expected-wheel-count", type=int)
    parser.add_argument("--require-sdist", action="store_true")
    return parser.parse_args()


def _validate_wheels(wheels: list[Path], versions: set[str]) -> None:
    """Validate wheel naming, ABI, and installed package contents."""
    for wheel in wheels:
        match = WHEEL.match(wheel.name)
        if match is None:
            raise RuntimeError(f"Unexpected wheel name: {wheel.name}")
        versions.add(match.group("version"))
        if "-abi3-" not in wheel.name:
            raise RuntimeError(f"Wheel isn't abi3: {wheel.name}")
        with zipfile.ZipFile(wheel) as archive:
            names = set(archive.namelist())
        missing = REQUIRED_WHEEL_FILES - names
        if missing:
            raise RuntimeError(f"{wheel.name} missing package files: {sorted(missing)}")
        if not any(
            name.startswith("neatxlsx/_native")
            and name.endswith((".so", ".pyd", ".dylib"))
            for name in names
        ):
            raise RuntimeError(f"{wheel.name} missing neatxlsx._native")


def _validate_sdist(sdist: Path, versions: set[str]) -> None:
    """Validate source-distribution naming and build-critical source contents."""
    match = SDIST.match(sdist.name)
    if match is None:
        raise RuntimeError(f"Unexpected sdist name: {sdist.name}")
    versions.add(match.group("version"))
    with tarfile.open(sdist, "r:gz") as archive:
        names = {name.split("/", 1)[1] for name in archive.getnames() if "/" in name}
    missing = REQUIRED_SDIST_FILES - names
    if missing:
        raise RuntimeError(f"{sdist.name} missing source files: {sorted(missing)}")


def validate(
    dist_dir: Path,
    expected_version: str | None = None,
    *,
    expected_wheel_count: int | None = None,
    require_sdist: bool = False,
) -> str:
    """Validate a wheel set and optional sdist as one versioned release bundle.

    Args:
        dist_dir: Directory containing neatxlsx distribution files.
        expected_version: Version that every distribution must contain.
        expected_wheel_count: Exact number of platform wheels required.
        require_sdist: Whether exactly one source distribution is required.

    Returns:
        The single version shared by all validated distributions.

    Raises:
        RuntimeError: If files are missing, malformed, incomplete, or have
            inconsistent versions.

    Examples:
        Validate the distributions produced by a local package build:

        >>> validate(Path("dist"), expected_version="0.1.0")
        '0.1.0'
    """
    wheels = sorted(dist_dir.glob("neatxlsx-*.whl"))
    if not wheels:
        raise RuntimeError(f"No neatxlsx wheel found in {dist_dir.resolve()}")
    if expected_wheel_count is not None and len(wheels) != expected_wheel_count:
        raise RuntimeError(
            f"Expected {expected_wheel_count} wheels, found {len(wheels)}"
        )

    versions: set[str] = set()
    _validate_wheels(wheels, versions)

    sdists = sorted(dist_dir.glob("neatxlsx-*.tar.gz"))
    if require_sdist and len(sdists) != 1:
        raise RuntimeError(f"Expected exactly one sdist, found {len(sdists)}")
    if len(sdists) > 1:
        raise RuntimeError(f"Expected at most one sdist, found {len(sdists)}")
    if sdists:
        _validate_sdist(sdists[0], versions)

    if len(versions) != 1:
        raise RuntimeError(f"Mixed distribution versions: {sorted(versions)}")
    version = next(iter(versions))
    if expected_version is not None and version != expected_version:
        raise RuntimeError(f"Expected version {expected_version!r}, found {version!r}")
    return version


def main() -> None:
    args = _parse_args()
    version = validate(
        args.dist_dir,
        args.expected_version,
        expected_wheel_count=args.expected_wheel_count,
        require_sdist=args.require_sdist,
    )
    print(f"Validated neatxlsx distribution version {version}")


if __name__ == "__main__":
    main()
