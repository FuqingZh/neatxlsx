from __future__ import annotations

import importlib.util
import sys
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
