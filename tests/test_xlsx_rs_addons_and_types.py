from __future__ import annotations

import zipfile
from pathlib import Path
from typing import Any

import polars as pl
import pytest

from neatxlsx import XlsxWriter  # noqa: E402
from neatxlsx._rs_bridge import is_rs_backend_available  # noqa: E402
from neatxlsx.spec import (  # noqa: E402
    SheetSlice,
    XlsxReport,
    XlsxValuePolicy,
    XlsxWriteOptions,
)


def test_xlsx_rs_report_types_align_spec(tmp_path: Path) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    path_file_out = tmp_path / "types.xlsx"

    with XlsxWriter(path_file_out) as inst_xlsx_writer:
        inst_xlsx_writer.write_sheet(pl.DataFrame({"a": [1, 2], "b": ["x", "y"]}), "S")
        reports = inst_xlsx_writer.report()

    assert isinstance(reports, tuple)
    assert len(reports) == 1
    assert isinstance(reports[0], XlsxReport)
    assert len(reports[0].sheets) == 1
    assert isinstance(reports[0].sheets[0], SheetSlice)


def test_xlsx_rs_writer_no_longer_accepts_addons(tmp_path: Path) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    with XlsxWriter(tmp_path / "no_addons.xlsx") as inst_xlsx_writer:
        writer_dynamic: Any = inst_xlsx_writer
        with pytest.raises(TypeError):
            writer_dynamic.write_sheet(
                pl.DataFrame({"a": [1]}),
                "S",
                addons=(),
            )


def test_xlsx_rs_writer_accepts_should_keywords(tmp_path: Path) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    path_file_out = tmp_path / "should_keywords.xlsx"
    df = pl.DataFrame({"a": [1, None], "b": ["x", "y"]})

    with XlsxWriter(path_file_out) as inst_xlsx_writer:
        result = inst_xlsx_writer.write_sheet(
            df,
            "S",
            should_merge_header=True,
            should_keep_missing_values=True,
        )

    assert result is inst_xlsx_writer
    assert path_file_out.exists()


def test_xlsx_rs_writer_accepts_num_frozen_keywords(tmp_path: Path) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    path_file_out = tmp_path / "frozen_keywords.xlsx"

    with XlsxWriter(path_file_out) as inst_xlsx_writer:
        result = inst_xlsx_writer.write_sheet(
            pl.DataFrame({"a": [1], "b": [2]}),
            "S",
            num_frozen_cols=1,
            num_frozen_rows=1,
        )

    assert result is inst_xlsx_writer
    assert path_file_out.exists()


def test_xlsx_writer_rejects_non_polars_body(tmp_path: Path) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    with XlsxWriter(tmp_path / "bad_body.xlsx") as inst_xlsx_writer:
        with pytest.raises(
            TypeError, match="body must be a polars DataFrame or LazyFrame"
        ):
            inst_xlsx_writer.write_sheet({"a": [1]}, "S")  # type: ignore[arg-type]


def test_xlsx_writer_rejects_non_dataframe_header(tmp_path: Path) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    body = pl.DataFrame({"a": [1]})
    with XlsxWriter(tmp_path / "bad_header_mapping.xlsx") as inst_xlsx_writer:
        with pytest.raises(
            TypeError, match="header must be a polars DataFrame or None"
        ):
            inst_xlsx_writer.write_sheet(
                body,
                "S",
                header={"a": ["meta"]},  # type: ignore[arg-type]
            )

    with XlsxWriter(tmp_path / "bad_header_sequence.xlsx") as inst_xlsx_writer:
        with pytest.raises(
            TypeError, match="header must be a polars DataFrame or None"
        ):
            inst_xlsx_writer.write_sheet(
                body,
                "S",
                header=[["meta"]],  # type: ignore[arg-type]
            )


def test_xlsx_write_options_accepts_should_prefixed_flags() -> None:
    cfg_write_options = XlsxWriteOptions(
        should_use_zip64=False,
        should_keep_missing_values=True,
        should_infer_numeric_cols=False,
        should_infer_integer_cols=False,
    )

    assert XlsxWriteOptions().should_use_zip64 is True
    assert cfg_write_options.should_use_zip64 is False
    assert cfg_write_options.should_keep_missing_values is True
    assert cfg_write_options.should_infer_numeric_cols is False
    assert cfg_write_options.should_infer_integer_cols is False


def test_xlsx_write_options_preserves_existing_positional_arguments() -> None:
    value_policy = XlsxValuePolicy()
    cfg_write_options = XlsxWriteOptions(value_policy, True, False, False)

    assert cfg_write_options.value_policy is value_policy
    assert cfg_write_options.should_keep_missing_values is True
    assert cfg_write_options.should_infer_numeric_cols is False
    assert cfg_write_options.should_infer_integer_cols is False
    assert cfg_write_options.should_use_zip64 is True


def test_xlsx_writer_accepts_options_write_keyword(tmp_path: Path) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    path_file_out = tmp_path / "options_write.xlsx"
    cfg_write_options = XlsxWriteOptions(should_keep_missing_values=True)

    with XlsxWriter(path_file_out, options_write=cfg_write_options) as inst_xlsx_writer:
        inst_xlsx_writer.write_sheet(pl.DataFrame({"a": [1, None]}), "S")

    assert path_file_out.exists()


def test_xlsx_writer_uses_zip64_by_default_and_allows_opt_out(
    tmp_path: Path,
) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    path_file_default = tmp_path / "zip64_default.xlsx"
    with XlsxWriter(path_file_default) as writer:
        writer.write_sheet(pl.DataFrame({"a": [1]}), "S")

    path_file_classic = tmp_path / "zip64_disabled.xlsx"
    with XlsxWriter(
        path_file_classic,
        options_write=XlsxWriteOptions(should_use_zip64=False),
    ) as writer:
        writer.write_sheet(pl.DataFrame({"a": [1]}), "S")

    with zipfile.ZipFile(path_file_default) as archive:
        assert min(info.extract_version for info in archive.infolist()) >= 45
    with zipfile.ZipFile(path_file_classic) as archive:
        assert max(info.extract_version for info in archive.infolist()) < 45


def test_xlsx_writer_rejects_legacy_write_options_keyword(tmp_path: Path) -> None:
    if not is_rs_backend_available():
        pytest.skip("Rust xlsx backend is unavailable")

    writer_cls_dynamic: Any = XlsxWriter
    with pytest.raises(TypeError):
        writer_cls_dynamic(
            tmp_path / "legacy_write_options.xlsx", write_options=XlsxWriteOptions()
        )
