from __future__ import annotations

from pathlib import Path
from typing import Any

import polars as pl
import pytest

from neatxlsx import _native  # noqa: E402
from neatxlsx import XlsxWriter
from neatxlsx._rs_bridge import (  # noqa: E402
    EXPECTED_BRIDGE_ABI,
    EXPECTED_BRIDGE_CONTRACT,
    create_xlsx_writer_via_rs,
    is_rs_backend_available,
)


def _create_rs_writer(file_out: Path) -> Any:
    writer = create_xlsx_writer_via_rs(str(file_out))
    assert writer is not None
    return writer


def test_xlsx_rs_bridge_smoke(tmp_path: Path) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    path_file_out = tmp_path / "smoke_rs.xlsx"

    with _create_rs_writer(path_file_out) as inst_xlsx_writer:
        inst_xlsx_writer.write_sheet(
            pl.DataFrame({"a": [1, 2], "b": ["x", "y"]}),
            "Sheet1",
        )
        reports = inst_xlsx_writer.report()

    assert path_file_out.exists()
    assert path_file_out.stat().st_size > 0
    assert len(reports) == 1
    assert len(reports[0].sheets) == 1
    assert reports[0].warnings == []


def test_xlsx_rs_bridge_write_sheet_batches_accepts_arrow_stream_source(
    tmp_path: Path,
) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    path_file_out = tmp_path / "lazy_batches_stream.xlsx"
    lf = pl.LazyFrame({"a": [1, 2, 3], "b": ["x", "y", "z"]})

    with _create_rs_writer(path_file_out) as inst_xlsx_writer:
        inst_xlsx_writer.write_sheet_batches(
            lf.collect_batches(chunk_size=2),
            lf.collect_batches(chunk_size=2),
            "Sheet1",
        )
        reports = inst_xlsx_writer.report()

    assert path_file_out.exists()
    assert path_file_out.stat().st_size > 0
    assert len(reports) == 1
    assert len(reports[0].sheets) == 1


def test_xlsx_rs_bridge_write_sheet_batches_accepts_empty_stream_with_schema_body(
    tmp_path: Path,
) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    path_file_out = tmp_path / "empty_batches_stream.xlsx"
    schema_body = pl.DataFrame(schema={"a": pl.Int64, "b": pl.Utf8})
    lf = schema_body.lazy()

    with _create_rs_writer(path_file_out) as inst_xlsx_writer:
        inst_xlsx_writer.write_sheet_batches(
            lf.collect_batches(chunk_size=2),
            lf.collect_batches(chunk_size=2),
            "Sheet1",
            schema_body=schema_body,
        )
        reports = inst_xlsx_writer.report()

    assert path_file_out.exists()
    assert len(reports) == 1
    assert len(reports[0].sheets) == 1
    assert reports[0].sheets[0].row_end_exclusive == 0
    assert reports[0].sheets[0].col_end_exclusive == 2


def test_xlsx_rs_bridge_single_pass_accepts_empty_stream_with_schema_body(
    tmp_path: Path,
) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    path_file_out = tmp_path / "empty_single_pass_stream.xlsx"
    schema_body = pl.DataFrame(schema={"a": pl.Int64, "b": pl.Utf8})

    with _create_rs_writer(path_file_out) as inst_xlsx_writer:
        inst_xlsx_writer.write_sheet_batches_single_pass(
            schema_body.lazy().collect_batches(chunk_size=2),
            "Sheet1",
            schema_body=schema_body,
        )
        reports = inst_xlsx_writer.report()

    assert path_file_out.exists()
    assert len(reports) == 1
    assert len(reports[0].sheets) == 1
    assert reports[0].sheets[0].row_end_exclusive == 0
    assert reports[0].sheets[0].col_end_exclusive == 2


def test_xlsx_rs_bridge_empty_batch_stream_without_schema_body_still_errors(
    tmp_path: Path,
) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    path_file_out = tmp_path / "empty_batches_no_schema.xlsx"
    lf = pl.DataFrame(schema={"a": pl.Int64, "b": pl.Utf8}).lazy()

    with _create_rs_writer(path_file_out) as inst_xlsx_writer:
        with pytest.raises(
            ValueError,
            match="Cannot write sheet from an empty batch stream with unknown schema",
        ):
            inst_xlsx_writer.write_sheet_batches(
                lf.collect_batches(chunk_size=2),
                lf.collect_batches(chunk_size=2),
                "Sheet1",
            )


def test_xlsx_rs_bridge_empty_single_pass_stream_without_schema_body_still_errors(
    tmp_path: Path,
) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    path_file_out = tmp_path / "empty_single_pass_no_schema.xlsx"
    lf = pl.DataFrame(schema={"a": pl.Int64, "b": pl.Utf8}).lazy()

    with _create_rs_writer(path_file_out) as inst_xlsx_writer:
        with pytest.raises(
            ValueError,
            match="Cannot write sheet from an empty batch stream with unknown schema",
        ):
            inst_xlsx_writer.write_sheet_batches_single_pass(
                lf.collect_batches(chunk_size=2),
                "Sheet1",
            )


def test_xlsx_rs_bridge_profile_arrow_drain_counts_direct_stream() -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    lf = pl.LazyFrame({"a": [1, 2, 3], "b": ["x", "y", "z"]})
    profile = _native._profile_arrow_drain(
        lf.collect_batches(chunk_size=2)
    )

    assert profile.batches == 2
    assert profile.rows == 3
    assert profile.cols == 2
    assert profile.cells == 6


def test_xlsx_writer_accepts_lazyframe_input(tmp_path: Path) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    path_file_out = tmp_path / "lazy_input.xlsx"
    body = pl.LazyFrame({"a": [1, 2], "b": ["x", "y"]})

    with XlsxWriter(path_file_out) as inst_xlsx_writer:
        inst_xlsx_writer.write_sheet(body, "Sheet1")

    assert path_file_out.exists()


def test_xlsx_rs_bridge_write_sheet_batches_keeps_iterable_fallback(
    tmp_path: Path,
) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    class BatchIterable:
        def __init__(self, batches: list[pl.DataFrame]) -> None:
            self._batches = batches

        def __iter__(self):  # type: ignore[no-untyped-def]
            return iter(self._batches)

    path_file_out = tmp_path / "lazy_batches_iterable_fallback.xlsx"
    batches = [pl.DataFrame({"a": [1, 2], "b": ["x", "y"]})]

    with _create_rs_writer(path_file_out) as inst_xlsx_writer:
        inst_xlsx_writer.write_sheet_batches(
            BatchIterable(batches),
            BatchIterable(batches),
            "Sheet1",
        )
        reports = inst_xlsx_writer.report()

    assert path_file_out.exists()
    assert path_file_out.stat().st_size > 0
    assert len(reports) == 1
    assert len(reports[0].sheets) == 1


def test_xlsx_rs_bridge_contract_constants_match() -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    assert _native.__bridge_abi__ == EXPECTED_BRIDGE_ABI
    assert _native.__bridge_contract__ == EXPECTED_BRIDGE_CONTRACT
