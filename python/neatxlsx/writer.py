import os
import warnings
from collections.abc import Mapping, Sequence
from pathlib import Path
from types import TracebackType
from typing import Any, ClassVar, Literal, Protocol, Self, cast

import polars as pl

from ._rs_bridge import create_xlsx_writer_via_rs, is_rs_backend_available
from .constant import (
    DEFAULT_XLSX_FORMATS,
    DEFAULT_XLSX_WRITE_OPTIONS,
    LIT_FMT_KEYS,
    ColumnIdentifier,
)
from .spec import (
    AutofitPolicy,
    CellFormatPatch,
    ScientificPolicy,
    XlsxReport,
    XlsxWriteOptions,
)


class ProtocolXlsxWriterBackend(Protocol):
    def close(self) -> None: ...

    def report(self) -> tuple[XlsxReport, ...]: ...

    def write_sheet(
        self,
        body: Any,
        sheet_name: str,
        *,
        header: Any | None = None,
        cols_integer: Sequence[ColumnIdentifier] | None = None,
        cols_decimal: Sequence[ColumnIdentifier] | None | Literal[False] = None,
        num_frozen_cols: int = 0,
        num_frozen_rows: int | None = None,
        should_merge_header: bool = False,
        should_keep_missing_values: bool | None = None,
        policy_autofit: AutofitPolicy | None = None,
        policy_scientific: ScientificPolicy | None = None,
    ) -> Any: ...

    def write_sheet_batches(
        self,
        batches_scan: Any,
        batches_write: Any,
        sheet_name: str,
        *,
        header: Any | None = None,
        cols_integer: Sequence[ColumnIdentifier] | None = None,
        cols_decimal: Sequence[ColumnIdentifier] | None | Literal[False] = None,
        num_frozen_cols: int = 0,
        num_frozen_rows: int | None = None,
        should_merge_header: bool = False,
        should_keep_missing_values: bool | None = None,
        policy_autofit: AutofitPolicy | None = None,
        policy_scientific: ScientificPolicy | None = None,
        schema_body: Any | None = None,
    ) -> Any: ...

    def write_sheet_batches_single_pass(
        self,
        batches_write: Any,
        sheet_name: str,
        *,
        header: Any | None = None,
        cols_integer: Sequence[ColumnIdentifier] | None = None,
        cols_decimal: Sequence[ColumnIdentifier] | None | Literal[False] = None,
        num_frozen_cols: int = 0,
        num_frozen_rows: int | None = None,
        should_merge_header: bool = False,
        should_keep_missing_values: bool | None = None,
        policy_autofit: AutofitPolicy | None = None,
        policy_scientific: ScientificPolicy | None = None,
        schema_body: Any | None = None,
    ) -> Any: ...


class XlsxWriter:
    """Rust-backed XLSX writer.

    Public API is kept aligned with the previous Python implementation.
    The execution backend is always Rust (``neatxlsx._native``) and this class is a
    thin Python facade that preserves call signatures and return types.
    """

    DEFAULT_XLSX_FORMATS: ClassVar[Mapping[LIT_FMT_KEYS, CellFormatPatch]] = (
        DEFAULT_XLSX_FORMATS
    )
    DEFAULT_XLSX_WRITE_OPTIONS: ClassVar[XlsxWriteOptions] = DEFAULT_XLSX_WRITE_OPTIONS

    def __init__(
        self,
        file_out: os.PathLike[str] | str,
        *,
        fmt_text: CellFormatPatch | None = None,
        fmt_integer: CellFormatPatch | None = None,
        fmt_decimal: CellFormatPatch | None = None,
        fmt_scientific: CellFormatPatch | None = None,
        fmt_header: CellFormatPatch | None = None,
        options_write: XlsxWriteOptions | None = None,
    ):
        if not is_rs_backend_available():
            raise RuntimeError(
                "Rust xlsx backend is unavailable. Build/install `neatxlsx._native` first."
            )

        self.file_out = Path(file_out)
        self._options_write = (
            options_write if options_write is not None else DEFAULT_XLSX_WRITE_OPTIONS
        )
        self._writer: ProtocolXlsxWriterBackend = cast(
            ProtocolXlsxWriterBackend,
            create_xlsx_writer_via_rs(
                str(self.file_out),
                fmt_text=fmt_text,
                fmt_integer=fmt_integer,
                fmt_decimal=fmt_decimal,
                fmt_scientific=fmt_scientific,
                fmt_header=fmt_header,
                options_write=self._options_write,
            ),
        )

    def __enter__(self) -> "XlsxWriter":
        return self

    def __exit__(
        self, exc_type: type | None, exc: BaseException | None, tb: TracebackType | None
    ) -> None:
        self.close()

    def close(self) -> None:
        self._writer.close()

    def report(self) -> tuple[XlsxReport, ...]:
        return self._writer.report()

    def write_sheet(
        self,
        body: pl.DataFrame | pl.LazyFrame,
        sheet_name: str,
        *,
        header: pl.DataFrame | None = None,
        cols_integer: Sequence[ColumnIdentifier] | None = None,
        cols_decimal: Sequence[ColumnIdentifier] | None | Literal[False] = None,
        num_frozen_cols: int = 0,
        num_frozen_rows: int | None = None,
        should_merge_header: bool = False,
        should_keep_missing_values: bool | None = None,
        policy_autofit: AutofitPolicy | None = None,
        policy_scientific: ScientificPolicy | None = None,
    ) -> Self:
        """Write one worksheet to the workbook.

        Args:
            body: Polars DataFrame or LazyFrame to write. DataFrame inputs are
                converted to LazyFrame internally and use the same streaming
                writer path as LazyFrame inputs.
            sheet_name: Requested worksheet name before Excel sanitization and
                uniqueness adjustments.
            header: Optional custom header grid as a Polars DataFrame. When
                provided, it must have the same width as ``body`` and at least
                one row.
            cols_integer:
                Optional column identifiers that should use integer formatting and
                integer conversion rules. Use ``str`` for literal column names and
                ``int`` for zero-based column indices. Pure numeric strings such as
                ``"0"`` are treated as column names, not indices.
            cols_decimal:
                Optional column identifiers that should use decimal formatting.
                Use ``str`` for literal column names and ``int`` for zero-based
                column indices. Pure numeric strings such as ``"0"`` are treated as
                column names, not indices.
                Pass ``False`` to disable explicit decimal-column selection.
            num_frozen_cols: Number of leftmost columns to freeze.
            num_frozen_rows: Number of top rows to freeze. When ``None``, the
                backend uses the resolved header height.
            should_merge_header:
                - ``True``: Merge all adjacent header labels that are identical.
                - ``False``: Don't merge any header labels.
            should_keep_missing_values:
                - ``True``: Write missing, NaN, and Inf values as text tokens.
                - ``False``: Write missing, NaN, and Inf values as blank cells.
                - ``None``: Use the writer-level option for missing value handling.
            policy_autofit: Column autofit policy applied to the sheet.
            policy_scientific: Scientific-number formatting policy applied per
                cell. If ``None``, scientific formatting is disabled by default.
                Only numeric values that fall within the policy scope and
                trigger thresholds use the scientific format; other cells keep
                the column base format.

        Returns:
            Self: The current writer instance for fluent chaining.

        Examples:
            ```python
            with XlsxWriter("output.xlsx") as writer:
                writer.write_sheet(
                    my_dataframe,
                    "Data",
                    cols_integer=["id", "age"],
                    cols_decimal=["score"],
                    num_frozen_cols=1,
                    should_merge_header=True,
                )

            with XlsxWriter("output.xlsx") as writer:
                writer.write_sheet(
                    my_dataframe,
                    "Data",
                    cols_integer=[0, 1],  # using column indices instead of names
                    cols_decimal=[2],
                    num_frozen_rows=2,
                    should_keep_missing_values=True,
                    policy_autofit=AutofitPolicy(
                        mode="all",
                        height_body_inferred_max=20_000,
                        width_cell_min=8,
                        width_cell_max=60,
                        width_cell_padding=2
                    ),
                    policy_scientific=ScientificPolicy(
                        scope="decimal",
                        thr_min=0.0001,
                        thr_max=1_000_000_000_000.0
                    ),
                )
            ```
        """
        _warn_numeric_string_column_selectors(cols_integer, arg_name="cols_integer")
        _warn_numeric_string_column_selectors(cols_decimal, arg_name="cols_decimal")
        body_lazy = _normalize_body(body)
        header_normalized = _normalize_header(header)
        schema_body = _derive_schema_body(body_lazy)

        chunk_size = _derive_collect_batches_chunk_size(
            body_lazy, options_write=self._options_write
        )
        if _can_write_lazy_single_pass(policy_autofit):
            self._writer.write_sheet_batches_single_pass(
                batches_write=_collect_batches(body_lazy, chunk_size=chunk_size),
                sheet_name=sheet_name,
                header=header_normalized,
                cols_integer=cols_integer,
                cols_decimal=cols_decimal,
                num_frozen_cols=num_frozen_cols,
                num_frozen_rows=num_frozen_rows,
                should_merge_header=should_merge_header,
                should_keep_missing_values=should_keep_missing_values,
                policy_autofit=policy_autofit,
                policy_scientific=policy_scientific,
                schema_body=schema_body,
            )
        else:
            self._writer.write_sheet_batches(
                batches_scan=_collect_batches(body_lazy, chunk_size=chunk_size),
                batches_write=_collect_batches(body_lazy, chunk_size=chunk_size),
                sheet_name=sheet_name,
                header=header_normalized,
                cols_integer=cols_integer,
                cols_decimal=cols_decimal,
                num_frozen_cols=num_frozen_cols,
                num_frozen_rows=num_frozen_rows,
                should_merge_header=should_merge_header,
                should_keep_missing_values=should_keep_missing_values,
                policy_autofit=policy_autofit,
                policy_scientific=policy_scientific,
                schema_body=schema_body,
            )
        return self


def _normalize_body(value: pl.DataFrame | pl.LazyFrame) -> pl.LazyFrame:
    if isinstance(value, pl.LazyFrame):
        return value
    if isinstance(value, pl.DataFrame):
        return value.lazy()
    raise TypeError("body must be a polars DataFrame or LazyFrame.")


def _normalize_header(value: pl.DataFrame | None) -> pl.DataFrame | None:
    if value is None or isinstance(value, pl.DataFrame):
        return value
    raise TypeError("header must be a polars DataFrame or None.")


def _derive_schema_body(value: pl.LazyFrame) -> pl.DataFrame:
    return pl.DataFrame(schema=value.collect_schema())


def _can_write_lazy_single_pass(policy_autofit: AutofitPolicy | None) -> bool:
    if policy_autofit is None:
        return True
    return policy_autofit.mode in {"header", "none"}


def _collect_batches(value: Any, *, chunk_size: int) -> Any:
    try:
        return value.collect_batches(chunk_size=chunk_size)
    except TypeError:
        return value.collect_batches()


def _derive_collect_batches_chunk_size(
    value: Any, *, options_write: XlsxWriteOptions
) -> int:
    width = _derive_lazy_width(value)
    policy = options_write.row_chunk_policy

    if policy.fixed_size is not None:
        chunk_size = policy.fixed_size
    elif width >= policy.width_large:
        chunk_size = policy.size_large
    elif width >= policy.width_medium:
        chunk_size = policy.size_medium
    else:
        chunk_size = policy.size_default

    if chunk_size < 1:
        raise ValueError("row_chunk_policy resolved to 0 rows; expected >= 1.")
    return chunk_size


def _derive_lazy_width(value: Any) -> int:
    collect_schema = getattr(value, "collect_schema", None)
    if callable(collect_schema):
        return len(cast(Any, collect_schema()))

    schema = getattr(value, "schema", None)
    if schema is not None:
        try:
            return len(cast(Any, schema))
        except TypeError:
            return 0

    return 0


def _warn_numeric_string_column_selectors(
    value: Sequence[ColumnIdentifier] | None | Literal[False] | object,
    *,
    arg_name: str,
) -> None:
    match value:
        case None | False:
            return
        case str() | int():
            items = (value,)
        case Sequence():
            items = value
        case _:
            return

    for _item in items:
        if isinstance(_item, str) and _item.isascii() and _item.isdigit():
            warnings.warn(
                (
                    f"{arg_name} contains numeric string selector {_item!r}; "
                    "string selectors are treated as literal column names. "
                    "Pass an int to select a zero-based column index."
                ),
                category=UserWarning,
                stacklevel=2,
            )
