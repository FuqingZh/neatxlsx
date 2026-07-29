from __future__ import annotations

import importlib.util
import io
import tarfile
import zipfile
from pathlib import Path

import pytest

SCRIPT = Path(__file__).parents[2] / "scripts" / "validate_dist.py"
SPEC = importlib.util.spec_from_file_location("validate_dist", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
validate_dist = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(validate_dist)

REQUIRED_SDIST_FILES = validate_dist.REQUIRED_SDIST_FILES
REQUIRED_WHEEL_FILES = validate_dist.REQUIRED_WHEEL_FILES
validate = validate_dist.validate


def _write_wheel(dist_dir: Path, version: str = "0.1.0") -> None:
    wheel = dist_dir / f"neatxlsx-{version}-cp311-abi3-manylinux_2_28_x86_64.whl"
    with zipfile.ZipFile(wheel, "w") as archive:
        for name in REQUIRED_WHEEL_FILES:
            archive.writestr(name, "")
        archive.writestr("neatxlsx/_native.abi3.so", "")


def _write_sdist(dist_dir: Path, version: str = "0.1.0") -> None:
    sdist = dist_dir / f"neatxlsx-{version}.tar.gz"
    with tarfile.open(sdist, "w:gz") as archive:
        for name in REQUIRED_SDIST_FILES:
            data = b""
            info = tarfile.TarInfo(f"neatxlsx-{version}/{name}")
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))


def test_validate_accepts_one_versioned_release_bundle(tmp_path: Path) -> None:
    _write_wheel(tmp_path)
    _write_sdist(tmp_path)

    assert (
        validate(
            tmp_path,
            expected_version="0.1.0",
            expected_wheel_count=1,
            require_sdist=True,
        )
        == "0.1.0"
    )


def test_validate_includes_sdist_version_in_consistency_check(
    tmp_path: Path,
) -> None:
    _write_wheel(tmp_path, "0.1.0")
    _write_sdist(tmp_path, "0.2.0")

    with pytest.raises(RuntimeError, match="Mixed distribution versions"):
        validate(tmp_path, require_sdist=True)


def test_validate_requires_the_expected_number_of_wheels(tmp_path: Path) -> None:
    _write_wheel(tmp_path)

    with pytest.raises(RuntimeError, match="Expected 5 wheels, found 1"):
        validate(tmp_path, expected_wheel_count=5)


def test_validate_can_require_one_sdist(tmp_path: Path) -> None:
    _write_wheel(tmp_path)

    with pytest.raises(RuntimeError, match="Expected exactly one sdist, found 0"):
        validate(tmp_path, require_sdist=True)
