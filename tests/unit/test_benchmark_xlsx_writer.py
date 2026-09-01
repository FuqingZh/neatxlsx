from __future__ import annotations

import importlib.util
import sys
import zipfile
from pathlib import Path
from types import ModuleType

import pytest

SCRIPT = Path(__file__).parents[2] / "scripts" / "benchmark_xlsx_writer.py"
SPEC = importlib.util.spec_from_file_location("benchmark_xlsx_writer", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
benchmark = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = benchmark
SPEC.loader.exec_module(benchmark)


class NativeModule(ModuleType):
    __build_profile__: str


def test_release_guard_rejects_debug_backend_outside_target(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    module = NativeModule("neatxlsx._native")
    module.__file__ = "/workspace/python/neatxlsx/_native.abi3.so"
    module.__build_profile__ = "debug"
    monkeypatch.setattr(benchmark.importlib, "import_module", lambda _name: module)

    with pytest.raises(RuntimeError, match="debug"):
        benchmark.enforce_release_rust_backend()


def test_release_guard_accepts_release_backend_outside_target(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    module = NativeModule("neatxlsx._native")
    module.__file__ = "/workspace/python/neatxlsx/_native.abi3.so"
    module.__build_profile__ = "release"
    monkeypatch.setattr(benchmark.importlib, "import_module", lambda _name: module)

    assert benchmark.enforce_release_rust_backend() == (
        benchmark.Path("/workspace/python/neatxlsx/_native.abi3.so")
    )


def test_release_guard_requires_embedded_build_profile(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    module = ModuleType("neatxlsx._native")
    module.__file__ = "/workspace/python/neatxlsx/_native.abi3.so"
    monkeypatch.setattr(benchmark.importlib, "import_module", lambda _name: module)

    with pytest.raises(RuntimeError, match="does not report"):
        benchmark.enforce_release_rust_backend()


@pytest.mark.parametrize(
    ("value", "expected"),
    [
        ("0", {0}),
        ("0-2,5", {0, 1, 2, 5}),
        (" 3 , 7-8 ", {3, 7, 8}),
    ],
)
def test_parse_cpu_list(value: str, expected: set[int]) -> None:
    assert benchmark._parse_cpu_list(value) == expected


@pytest.mark.parametrize("value", ["", "1,", "3-1", "-1"])
def test_parse_cpu_list_rejects_invalid_values(value: str) -> None:
    with pytest.raises((ValueError, TypeError)):
        benchmark._parse_cpu_list(value)


def test_require_path_within_rejects_sibling(tmp_path: Path) -> None:
    root = tmp_path / "root"
    sibling = tmp_path / "sibling" / "module.py"
    root.mkdir()
    sibling.parent.mkdir()
    sibling.touch()

    with pytest.raises(RuntimeError, match="must be inside"):
        benchmark._require_path_within(sibling, root, "module")


def test_zip_member_manifest_hashes_uncompressed_content(tmp_path: Path) -> None:
    archive_path = tmp_path / "sample.xlsx"
    with zipfile.ZipFile(
        archive_path, "w", compression=zipfile.ZIP_DEFLATED
    ) as archive:
        archive.writestr("b.txt", b"second")
        archive.writestr("a.txt", b"first")

    assert benchmark.build_zip_member_manifest(archive_path) == [
        {
            "name": "a.txt",
            "size": 5,
            "sha256": "a7937b64b8caa58f03721bb6bacf5c78cb235febe0e70b1b84cd99541461a08e",
            "comparison_sha256": "a7937b64b8caa58f03721bb6bacf5c78cb235febe0e70b1b84cd99541461a08e",
            "normalization": None,
        },
        {
            "name": "b.txt",
            "size": 6,
            "sha256": "16367aacb67a4a017c8da8ab95682ccb390863780f7114dda0a0e0c55644c7c4",
            "comparison_sha256": "16367aacb67a4a017c8da8ab95682ccb390863780f7114dda0a0e0c55644c7c4",
            "normalization": None,
        },
    ]


def test_core_properties_manifest_normalizes_only_generated_timestamps(
    tmp_path: Path,
) -> None:
    manifests = []
    for index, timestamp in enumerate(("2026-01-01T00:00:00Z", "2026-02-02T00:00:00Z")):
        archive_path = tmp_path / f"sample-{index}.xlsx"
        core = (
            '<dcterms:created xsi:type="dcterms:W3CDTF">'
            f"{timestamp}</dcterms:created>"
            '<dcterms:modified xsi:type="dcterms:W3CDTF">'
            f"{timestamp}</dcterms:modified>"
        )
        with zipfile.ZipFile(archive_path, "w") as archive:
            archive.writestr("docProps/core.xml", core)
        manifests.append(benchmark.build_zip_member_manifest(archive_path)[0])

    assert manifests[0]["sha256"] != manifests[1]["sha256"]
    assert manifests[0]["comparison_sha256"] == manifests[1]["comparison_sha256"]
    assert manifests[0]["normalization"] == "core-created-modified-utc"
