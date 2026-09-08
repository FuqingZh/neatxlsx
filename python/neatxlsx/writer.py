"""Transactional Python facade for the Rust XLSX writer."""

from __future__ import annotations

import os
import stat
import tempfile
import threading
import warnings
from collections.abc import Mapping, Sequence
from dataclasses import fields
from pathlib import Path
from types import TracebackType
from typing import Any, Literal, Protocol, Self, cast

import polars as pl
import polars.selectors as cs

from ._dtype import prepare_lazy_frame
from ._polars import collect_batches
from ._rs_bridge import create_xlsx_writer_via_rs, is_rs_backend_available
from .errors import CommitError, StateError, WriteError
from .spec import (
    Autofit,
    Format,
    ScientificNotation,
    SheetReport,
    WorksheetPart,
    XlsxReport,
    _RowChunkPolicy,
    _ValuePolicy,
    _WriteOptions,
)

ColumnSelection = str | int | Sequence[str | int] | cs.Selector | None
_UMASK_LOCK = threading.Lock()


class _Backend(Protocol):
    def close(self) -> None: ...

    def report(self) -> tuple[XlsxReport, ...]: ...

    def write_sheet_batches(
        self,
        batches: Any,
        sheet_name: str,
        **kwargs: Any,
    ) -> Any: ...


class Workbook:
    """Write Polars tables to one XLSX file with transactional replacement.

    The target is changed only by a successful :meth:`close`. ZIP64 is enabled
    by default for large-workbook reliability; pass ``use_zip64=False`` only
    for readers that don't support ZIP64.

    Args:
        path: Local or mounted-filesystem output path. Its parent must exist.
        text_format: Partial override for text cells.
        integer_format: Partial override for integer cells.
        decimal_format: Partial override for decimal cells.
        scientific_format: Partial override for scientific-number cells.
        header_format: Partial override for header cells.
        keep_missing_values: Write missing and non-finite values as tokens
            instead of blank cells.
        infer_numeric_columns: Infer numeric formatting from Polars dtypes.
        infer_integer_columns: Infer integer formatting from Polars dtypes.
        use_zip64: Use ZIP64 container extensions.
        integer_coerce: Preserve non-integral values as text (``"strict"``) or
            truncate them to integers (``"coerce"``).
        missing_value: Token for null values when missing values are retained.
        nan_value: Token for NaN.
        positive_infinity: Token for positive infinity.
        negative_infinity: Token for negative infinity.
        chunk_size: Fixed number of LazyFrame rows per Arrow batch.

    Examples:
        >>> import neatxlsx as nx
        >>> import polars as pl
        >>> with nx.Workbook("report.xlsx") as workbook:
        ...     workbook.write_sheet(pl.DataFrame({"value": [1, 2]}), "Data")
    """

    def __init__(
        self,
        path: str | os.PathLike[str],
        *,
        text_format: Format | None = None,
        integer_format: Format | None = None,
        decimal_format: Format | None = None,
        scientific_format: Format | None = None,
        header_format: Format | None = None,
        keep_missing_values: bool = False,
        infer_numeric_columns: bool = True,
        infer_integer_columns: bool = True,
        use_zip64: bool = True,
        integer_coerce: Literal["strict", "coerce"] = "strict",
        missing_value: str = "NA",
        nan_value: str = "NaN",
        positive_infinity: str = "Inf",
        negative_infinity: str = "-Inf",
        chunk_size: int | None = None,
    ) -> None:
        if isinstance(path, bool) or not isinstance(path, (str, os.PathLike)):
            raise TypeError("path must be str or os.PathLike[str].")
        if integer_coerce not in {"strict", "coerce"}:
            raise ValueError("integer_coerce must be 'strict' or 'coerce'.")
        if chunk_size is not None and chunk_size < 1:
            raise ValueError("chunk_size must be >= 1 or None.")
        for name, value in (
            ("keep_missing_values", keep_missing_values),
            ("infer_numeric_columns", infer_numeric_columns),
            ("infer_integer_columns", infer_integer_columns),
            ("use_zip64", use_zip64),
        ):
            if not isinstance(value, bool):
                raise TypeError(f"{name} must be bool.")

        for name, value in (
            ("text_format", text_format),
            ("integer_format", integer_format),
            ("decimal_format", decimal_format),
            ("scientific_format", scientific_format),
            ("header_format", header_format),
        ):
            if value is not None and not isinstance(value, Format):
                raise TypeError(f"{name} must be neatxlsx.Format or None.")

        target = Path(path)
        if target.is_symlink():
            raise ValueError("path must not be a symbolic link.")
        if target.exists() and not target.is_file():
            raise ValueError("path must name a file, not a directory.")
        parent = target.parent if target.parent != Path("") else Path(".")
        if not parent.exists():
            raise FileNotFoundError(f"Output parent does not exist: {parent}")
        if not parent.is_dir():
            raise NotADirectoryError(f"Output parent is not a directory: {parent}")
        if not is_rs_backend_available():
            raise RuntimeError(
                "Rust XLSX backend is unavailable; reinstall a platform wheel."
            )

        self.path = target
        self._target_mode = (
            stat.S_IMODE(target.stat().st_mode)
            if target.exists()
            else 0o666 & ~_read_umask()
        )
        descriptor, temp_name = tempfile.mkstemp(
            prefix=f".{target.name}.neatxlsx-",
            suffix=".xlsx",
            dir=parent,
        )
        os.close(descriptor)
        self._temp_path = Path(temp_name)
        self._state: Literal["open", "prepared", "closed", "aborted"] = "open"
        self._requested_names: list[str] = []
        self._reports_cache: tuple[SheetReport, ...] = ()
        self._infer_numeric_columns = infer_numeric_columns
        self._infer_integer_columns = infer_integer_columns

        options = _WriteOptions(
            value_policy=_ValuePolicy(
                missing_value_str=missing_value,
                nan_str=nan_value,
                posinf_str=positive_infinity,
                neginf_str=negative_infinity,
                integer_coerce=integer_coerce,
            ),
            should_keep_missing_values=keep_missing_values,
            # Python resolves dtype inference per write, including overrides.
            should_infer_numeric_cols=False,
            should_infer_integer_cols=False,
            row_chunk_policy=_RowChunkPolicy(fixed_size=chunk_size),
            should_use_zip64=use_zip64,
        )
        self._options = options
        try:
            self._backend: _Backend | None = cast(
                _Backend,
                create_xlsx_writer_via_rs(
                    str(self._temp_path),
                    fmt_text=text_format,
                    fmt_integer=integer_format,
                    fmt_decimal=decimal_format,
                    fmt_scientific=scientific_format,
                    fmt_header=header_format,
                    options_write=options,
                ),
            )
        except Exception:
            self._state = "aborted"
            self._temp_path.unlink(missing_ok=True)
            raise

    def __enter__(self) -> Self:
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        traceback: TracebackType | None,
    ) -> None:
        if exc_type is None:
            self.close()
        else:
            self.abort()

    def __del__(self) -> None:
        if getattr(self, "_state", None) in {"open", "prepared"}:
            warnings.warn(
                "Unclosed neatxlsx.Workbook was discarded; call close() or use a context manager.",
                ResourceWarning,
                stacklevel=2,
            )
            try:
                self.abort()
            except Exception:
                pass

    def write_sheet(
        self,
        data: pl.DataFrame | pl.LazyFrame,
        sheet_name: str,
        *,
        header: pl.DataFrame | None = None,
        header_row_formats: Sequence[Format | None] | None = None,
        header_column_formats: Mapping[str | int, Format] | None = None,
        column_formats: Mapping[str | int, Format] | None = None,
        integer_columns: ColumnSelection = None,
        decimal_columns: ColumnSelection = None,
        freeze_columns: int = 0,
        freeze_rows: int | None = None,
        merge_header: bool = False,
        keep_missing_values: bool | None = None,
        autofit: Autofit | None = None,
        scientific_notation: ScientificNotation | None = None,
        infer_numeric_columns: bool | None = None,
        infer_integer_columns: bool | None = None,
    ) -> Self:
        """Write one logical table, splitting it at Excel limits when needed.

        Explicit integer or decimal selectors override inferred formatting for
        those columns. Other columns continue to use dtype inference. Selectors
        may be a column name, zero-based index, ordered sequence, or Polars
        selector. Empty and unordered collections are rejected.

        Args:
            data: Polars DataFrame or LazyFrame.
            sheet_name: Requested name before Excel sanitization and uniqueness.
            header: Optional multi-row header DataFrame with the same width.
            header_row_formats: Per-row patches for a custom header. Each item
                is a :class:`Format` patch or ``None`` to inherit the workbook
                header format. The sequence length must equal ``header.height``.
            header_column_formats: Per-column header :class:`Format` patches
                keyed by data-column name or zero-based index. These patches
                apply to generated and custom header rows after any row patch.
                Nonempty mappings cannot be combined with ``merge_header``.
            column_formats: Per-body-column :class:`Format` patches keyed by
                column name or zero-based index. These patches take precedence
                over inferred and workbook role formats.
            integer_columns: Columns forced to integer handling.
            decimal_columns: Columns forced to decimal handling.
            freeze_columns: Number of leading columns to freeze.
            freeze_rows: Number of leading rows to freeze; ``None`` uses header
                height.
            merge_header: Merge adjacent equal multi-row header labels.
            keep_missing_values: Per-sheet override; ``None`` inherits the
                workbook setting.
            autofit: Width inference policy.
            scientific_notation: Scientific-number policy.
            infer_numeric_columns: Per-sheet inference override.
            infer_integer_columns: Per-sheet integer-inference override.

        Returns:
            The same workbook for fluent chaining.

        Examples:
            >>> import polars as pl
            >>> import polars.selectors as cs
            >>> with Workbook("report.xlsx") as workbook:
            ...     workbook.write_sheet(
            ...         pl.DataFrame({"id": [1], "score": [1.25]}),
            ...         "Data",
            ...         header=pl.DataFrame(
            ...             {"id": ["Metadata", "Identifier"], "score": ["Result", "Score"]}
            ...         ),
            ...         header_row_formats=[Format(font_name="SimSun"), None],
            ...         header_column_formats={"score": Format(bold=True)},
            ...         column_formats={"score": Format(font_name="SimSun")},
            ...         integer_columns="id",
            ...         decimal_columns=cs.float(),
            ...         freeze_columns=1,
            ...     )
        """
        self._require_open()
        lazy = _normalize_data(data)
        normalized_header = _normalize_header(header)
        schema = lazy.collect_schema()
        if normalized_header is not None:
            if normalized_header.height == 0:
                raise ValueError("header must contain at least one row.")
            if normalized_header.width != len(schema):
                raise ValueError("header width must equal data width.")
        resolved_header_row_formats = _normalize_header_row_formats(
            normalized_header, header_row_formats
        )
        resolved_header_column_formats = _resolve_format_columns(
            header_column_formats, schema, "header_column_formats"
        )
        resolved_column_formats = _resolve_column_formats(column_formats, schema)
        resolved_autofit = autofit or Autofit()
        resolved_scientific = scientific_notation or ScientificNotation()
        _validate_nonnegative_int(freeze_columns, "freeze_columns")
        if freeze_rows is not None:
            _validate_nonnegative_int(freeze_rows, "freeze_rows")
        if not isinstance(sheet_name, str):
            raise TypeError("sheet_name must be str.")
        if keep_missing_values is not None and not isinstance(
            keep_missing_values, bool
        ):
            raise TypeError("keep_missing_values must be bool or None.")
        if not isinstance(merge_header, bool):
            raise TypeError("merge_header must be bool.")
        if merge_header and resolved_header_column_formats:
            raise ValueError(
                "header_column_formats cannot be nonempty when merge_header=True."
            )

        infer_numeric = _resolve_inherit_bool(
            infer_numeric_columns,
            self._infer_numeric_columns,
            "infer_numeric_columns",
        )
        infer_integer = _resolve_inherit_bool(
            infer_integer_columns,
            self._infer_integer_columns,
            "infer_integer_columns",
        )
        preparation = prepare_lazy_frame(lazy, schema)
        lazy = preparation.lazy
        value_plans = preparation.plans
        integer_names, decimal_names = _resolve_numeric_roles(
            schema,
            integer_columns=integer_columns,
            decimal_columns=decimal_columns,
            infer_numeric=infer_numeric,
            infer_integer=infer_integer,
        )
        chunk_size = _derive_chunk_size(
            len(schema), self._options.row_chunk_policy.fixed_size
        )
        schema_frame = pl.DataFrame(schema=lazy.collect_schema())
        kwargs = {
            "header": normalized_header,
            "header_row_formats": resolved_header_row_formats,
            "header_column_formats": resolved_header_column_formats,
            "column_formats": resolved_column_formats,
            "cols_integer": integer_names,
            "cols_decimal": decimal_names,
            "num_frozen_cols": freeze_columns,
            "num_frozen_rows": freeze_rows,
            "should_merge_header": merge_header,
            "should_keep_missing_values": keep_missing_values,
            "policy_autofit": resolved_autofit,
            "policy_scientific": resolved_scientific,
            "schema_body": schema_frame,
            "value_plans": [plan.to_bridge() for plan in value_plans],
        }
        backend = cast(_Backend, self._backend)
        try:
            backend.write_sheet_batches(
                collect_batches(lazy, chunk_size=chunk_size),
                sheet_name,
                **kwargs,
            )
        except Exception as exc:
            self.abort()
            raise WriteError(f"Could not write sheet {sheet_name!r}: {exc}") from exc
        self._requested_names.append(sheet_name)
        return self

    def report(self) -> tuple[SheetReport, ...]:
        """Return immutable metadata for successful logical writes.

        Examples:
            >>> workbook = Workbook("report.xlsx")
            >>> workbook.write_sheet(pl.DataFrame({"a": [1]}), "Data")
            Workbook(...)
            >>> workbook.report()[0].requested_name
            'Data'
            >>> workbook.abort()
        """
        backend = self._backend
        if backend is None:
            return self._reports_cache
        return self._convert_reports(backend.report())

    def _convert_reports(
        self, reports: tuple[XlsxReport, ...]
    ) -> tuple[SheetReport, ...]:
        return tuple(
            SheetReport(
                requested_name=requested_name,
                worksheets=tuple(
                    WorksheetPart(
                        name=part.sheet_name,
                        row_start=part.row_start_inclusive,
                        row_stop=part.row_end_exclusive,
                        column_start=part.col_start_inclusive,
                        column_stop=part.col_end_exclusive,
                    )
                    for part in report.sheets
                ),
                warnings=tuple(report.warnings),
            )
            for requested_name, report in zip(
                self._requested_names, reports, strict=True
            )
        )

    def close(self) -> None:
        """Commit the completed workbook to its target.

        Repeated calls after success are harmless. If final replacement raises
        :class:`CommitError`, the completed temporary file is retained and the
        same method may be retried.

        Examples:
            >>> workbook = Workbook("report.xlsx")
            >>> workbook.write_sheet(pl.DataFrame({"a": [1]}), "Data")
            Workbook(...)
            >>> workbook.close()
            >>> workbook.close()
        """
        if self._state == "closed":
            return
        if self._state == "aborted":
            raise StateError("Cannot close an aborted workbook.")
        if self._state == "open":
            backend = cast(_Backend, self._backend)
            try:
                backend.close()
            except Exception as exc:
                self.abort()
                raise WriteError(f"Could not finalize XLSX content: {exc}") from exc
            self._reports_cache = self._convert_reports(backend.report())
            self._state = "prepared"
            self._backend = None
        try:
            os.chmod(self._temp_path, self._target_mode)
        except OSError as exc:
            raise CommitError(str(self.path), str(exc)) from exc
        try:
            os.replace(self._temp_path, self.path)
        except OSError as exc:
            raise CommitError(str(self.path), str(exc)) from exc
        self._state = "closed"

    def abort(self) -> None:
        """Discard the in-progress or prepared workbook.

        Repeated calls after abort are harmless.

        Examples:
            >>> workbook = Workbook("report.xlsx")
            >>> workbook.abort()
            >>> workbook.abort()
        """
        if self._state == "aborted":
            return
        if self._state == "closed":
            raise StateError("Cannot abort a committed workbook.")
        if self._backend is not None:
            self._reports_cache = self._convert_reports(self._backend.report())
        self._backend = None
        self._temp_path.unlink(missing_ok=True)
        self._state = "aborted"

    def _require_open(self) -> None:
        if self._state != "open":
            raise StateError(f"Cannot write when workbook state is {self._state!r}.")

    def __repr__(self) -> str:
        return f"Workbook(path={str(self.path)!r}, state={self._state!r})"


def _merge_format(default: Format, override: Format | None) -> Format:
    if override is None:
        return default
    if not isinstance(override, Format):
        raise TypeError("format overrides must be neatxlsx.Format or None.")
    changes = {
        field.name: value
        for field in fields(override)
        if (value := getattr(override, field.name)) is not None
    }
    return default.replace(**changes)


def _normalize_data(value: pl.DataFrame | pl.LazyFrame) -> pl.LazyFrame:
    if isinstance(value, pl.LazyFrame):
        return value
    if isinstance(value, pl.DataFrame):
        return value.lazy()
    raise TypeError("data must be a polars DataFrame or LazyFrame.")


def _normalize_header(value: pl.DataFrame | None) -> pl.DataFrame | None:
    if value is None or isinstance(value, pl.DataFrame):
        return value
    raise TypeError("header must be a polars DataFrame or None.")


def _normalize_header_row_formats(
    header: pl.DataFrame | None,
    value: Sequence[Format | None] | None,
) -> tuple[Format | None, ...]:
    if value is None:
        return ()
    if header is None:
        raise ValueError("header_row_formats requires a custom header.")
    if isinstance(
        value, (str, bytes, bytearray, set, frozenset, Mapping)
    ) or not isinstance(value, Sequence):
        raise TypeError("header_row_formats must be a sequence of Format or None.")
    if len(value) != header.height:
        raise ValueError("header_row_formats length must equal header height.")
    for item in value:
        if item is not None and not isinstance(item, Format):
            raise TypeError("header_row_formats items must be neatxlsx.Format or None.")
    return tuple(value)


def _resolve_column_formats(
    value: Mapping[str | int, Format] | None,
    schema: pl.Schema,
) -> tuple[tuple[int, Format], ...]:
    return _resolve_format_columns(value, schema, "column_formats")


def _resolve_format_columns(
    value: Mapping[str | int, Format] | None,
    schema: pl.Schema,
    argument: str,
) -> tuple[tuple[int, Format], ...]:
    if value is None:
        return ()
    if not isinstance(value, Mapping):
        raise TypeError(
            f"{argument} must be a mapping of column name or index to Format."
        )

    names = schema.names()
    resolved: dict[int, Format] = {}
    for key, fmt in value.items():
        if isinstance(key, bool):
            raise TypeError(f"{argument} keys must not be bool.")
        if isinstance(key, str):
            if key not in schema:
                raise ValueError(f"{argument} contains unknown column {key!r}.")
            index = names.index(key)
        elif isinstance(key, int):
            if key < 0 or key >= len(names):
                raise ValueError(f"{argument} index {key} is out of range.")
            index = key
        else:
            raise TypeError(f"{argument} keys must be str or int.")
        if not isinstance(fmt, Format):
            raise TypeError(f"{argument} values must be neatxlsx.Format.")
        if index in resolved:
            raise ValueError(
                f"{argument} selects column {names[index]!r} more than once."
            )
        resolved[index] = fmt
    return tuple(sorted(resolved.items()))


def _resolve_numeric_roles(
    schema: pl.Schema,
    *,
    integer_columns: ColumnSelection,
    decimal_columns: ColumnSelection,
    infer_numeric: bool,
    infer_integer: bool,
) -> tuple[tuple[str, ...], tuple[str, ...]]:
    names = tuple(schema.names())
    explicit_integer = set(_resolve_columns(integer_columns, schema, "integer_columns"))
    explicit_decimal = set(_resolve_columns(decimal_columns, schema, "decimal_columns"))
    overlap = explicit_integer & explicit_decimal
    if overlap:
        raise ValueError(
            "integer_columns and decimal_columns overlap: " + ", ".join(sorted(overlap))
        )

    numeric = {
        name for name, dtype in schema.items() if infer_numeric and dtype.is_numeric()
    }
    integer = {
        name for name, dtype in schema.items() if infer_integer and dtype.is_integer()
    }
    numeric.update(integer)
    numeric.update(explicit_decimal)
    numeric.update(explicit_integer)
    integer.difference_update(explicit_decimal)
    integer.update(explicit_integer)
    return (
        tuple(name for name in names if name in integer),
        tuple(name for name in names if name in numeric and name not in integer),
    )


def _resolve_columns(
    value: ColumnSelection,
    schema: pl.Schema,
    argument: str,
) -> tuple[str, ...]:
    if value is None:
        return ()
    if isinstance(value, bool):
        raise TypeError(f"{argument} must not be bool.")
    if isinstance(value, cs.Selector):
        return tuple(cs.expand_selector(schema, value))
    if isinstance(value, (str, int)):
        items: Sequence[str | int] = (value,)
    elif isinstance(value, (set, frozenset, Mapping)):
        raise TypeError(
            f"{argument} must be an ordered sequence, not {type(value).__name__}."
        )
    elif isinstance(value, Sequence) and not isinstance(value, (bytes, bytearray)):
        if not value:
            raise ValueError(f"{argument} must not be empty; use None.")
        items = value
    else:
        raise TypeError(
            f"{argument} must be a name, index, ordered sequence, Polars selector, or None."
        )

    names = schema.names()
    resolved: list[str] = []
    for item in items:
        if isinstance(item, bool):
            raise TypeError(f"{argument} contains bool, which isn't a column index.")
        if isinstance(item, str):
            if item not in schema:
                raise ValueError(f"{argument} contains unknown column {item!r}.")
            name = item
        elif isinstance(item, int):
            if item < 0 or item >= len(names):
                raise ValueError(f"{argument} index {item} is out of range.")
            name = names[item]
        else:
            raise TypeError(f"{argument} items must be str or int.")
        if name not in resolved:
            resolved.append(name)
    return tuple(resolved)


def _resolve_inherit_bool(value: bool | None, default: bool, name: str) -> bool:
    if value is None:
        return default
    if not isinstance(value, bool):
        raise TypeError(f"{name} must be bool or None.")
    return value


def _validate_nonnegative_int(value: int, name: str) -> None:
    if isinstance(value, bool) or not isinstance(value, int):
        raise TypeError(f"{name} must be int.")
    if value < 0:
        raise ValueError(f"{name} must be >= 0.")


def _derive_chunk_size(width: int, fixed_size: int | None) -> int:
    if fixed_size is not None:
        return fixed_size
    if width >= 8_000:
        return 1_000
    if width >= 2_000:
        return 2_000
    return 10_000


def _read_umask() -> int:
    with _UMASK_LOCK:
        value = os.umask(0)
        os.umask(value)
        return value
