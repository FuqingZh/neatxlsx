"""Public value objects and private bridge models."""

from __future__ import annotations

from dataclasses import dataclass, fields, replace
from typing import Any, Literal, Self, cast


class _Unset:
    __slots__ = ()


_UNSET = _Unset()


@dataclass(frozen=True, slots=True)
class Format:
    """A partial cell-format override.

    Fields left as ``None`` inherit the role default. Explicit values such as
    ``False``, ``0``, or ``"general"`` replace that default.

    Examples:
        >>> base = Format(bold=True, num_format="0.00")
        >>> base.replace(bold=False)
        Format(font_name=None, font_size=None, bold=False, italic=None, align=None, valign=None, border=None, text_wrap=None, top=None, bottom=None, left=None, right=None, num_format='0.00', bg_color=None, font_color=None)
    """

    font_name: str | None = None
    font_size: int | None = None
    bold: bool | None = None
    italic: bool | None = None
    align: str | None = None
    valign: str | None = None
    border: int | None = None
    text_wrap: bool | None = None
    top: int | None = None
    bottom: int | None = None
    left: int | None = None
    right: int | None = None
    num_format: str | None = None
    bg_color: str | None = None
    font_color: str | None = None

    def __post_init__(self) -> None:
        for name in ("bold", "italic", "text_wrap"):
            value = getattr(self, name)
            if value is not None and type(value) is not bool:
                raise TypeError(f"{name} must be bool or None.")

        if self.font_name is not None:
            if not isinstance(self.font_name, str):
                raise TypeError("font_name must be str or None.")
            if not self.font_name:
                raise ValueError("font_name must be nonempty.")
        if self.num_format is not None:
            if not isinstance(self.num_format, str):
                raise TypeError("num_format must be str or None.")
            if not self.num_format:
                raise ValueError("num_format must be nonempty.")

        if self.font_size is not None:
            if isinstance(self.font_size, bool) or not isinstance(self.font_size, int):
                raise TypeError("font_size must be an integer or None.")
            if not 1 <= self.font_size <= 409:
                raise ValueError("font_size must be between 1 and 409.")

        for name in ("border", "top", "bottom", "left", "right"):
            value = getattr(self, name)
            if value is None:
                continue
            if isinstance(value, bool) or not isinstance(value, int):
                raise TypeError(f"{name} must be an integer or None.")
            if not 0 <= value <= 13:
                raise ValueError(f"{name} must be between 0 and 13.")

        horizontal = {
            "general",
            "left",
            "center",
            "right",
            "fill",
            "justify",
            "center_across",
            "distributed",
        }
        vertical = {
            "top",
            "bottom",
            "vcenter",
            "vertical_center",
            "vjustify",
            "vertical_justify",
            "vdistributed",
            "vertical_distributed",
        }
        if self.align is not None:
            if not isinstance(self.align, str):
                raise TypeError("align must be str or None.")
            align = self.align.strip().lower()
            if align not in horizontal:
                raise ValueError("align has an unsupported value.")
            object.__setattr__(self, "align", align)
        if self.valign is not None:
            if not isinstance(self.valign, str):
                raise TypeError("valign must be str or None.")
            valign = self.valign.strip().lower()
            if valign not in vertical:
                raise ValueError("valign has an unsupported value.")
            object.__setattr__(self, "valign", valign)

        for name in ("bg_color", "font_color"):
            value = getattr(self, name)
            if value is None:
                continue
            if not isinstance(value, str):
                raise TypeError(f"{name} must be str or None.")
            normalized = value.strip().upper()
            if normalized.startswith("#"):
                normalized = normalized[1:]
            if len(normalized) != 6 or any(
                char not in "0123456789ABCDEF" for char in normalized
            ):
                raise ValueError(f"{name} must be a six-digit hexadecimal RGB color.")
            object.__setattr__(self, name, f"#{normalized}")

    def replace(
        self,
        *,
        font_name: str | None = cast(Any, _UNSET),
        font_size: int | None = cast(Any, _UNSET),
        bold: bool | None = cast(Any, _UNSET),
        italic: bool | None = cast(Any, _UNSET),
        align: str | None = cast(Any, _UNSET),
        valign: str | None = cast(Any, _UNSET),
        border: int | None = cast(Any, _UNSET),
        text_wrap: bool | None = cast(Any, _UNSET),
        top: int | None = cast(Any, _UNSET),
        bottom: int | None = cast(Any, _UNSET),
        left: int | None = cast(Any, _UNSET),
        right: int | None = cast(Any, _UNSET),
        num_format: str | None = cast(Any, _UNSET),
        bg_color: str | None = cast(Any, _UNSET),
        font_color: str | None = cast(Any, _UNSET),
    ) -> Self:
        """Return a copy with only the supplied fields replaced.

        Examples:
            >>> Format(bold=True).replace(bold=False, font_color="#666666")
            Format(font_name=None, font_size=None, bold=False, italic=None, align=None, valign=None, border=None, text_wrap=None, top=None, bottom=None, left=None, right=None, num_format=None, bg_color=None, font_color='#666666')
        """
        values = locals()
        changes = {
            field.name: values[field.name]
            for field in fields(self)
            if values[field.name] is not _UNSET
        }
        return replace(self, **changes)


@dataclass(frozen=True, slots=True)
class Autofit:
    """Control how worksheet column widths are inferred.

    ``body`` and ``all`` modes evaluate a LazyFrame twice; ``header`` and
    ``none`` retain the single-pass write path.

    Examples:
        >>> Autofit(mode="all", max_rows=5_000, max_width=48)
        Autofit(mode='all', max_rows=5000, min_width=8, max_width=48, padding=2)
    """

    mode: Literal["none", "header", "body", "all"] = "header"
    max_rows: int | None = 20_000
    min_width: int = 8
    max_width: int = 60
    padding: int = 2

    def __post_init__(self) -> None:
        if self.mode not in {"none", "header", "body", "all"}:
            raise ValueError("mode must be one of: 'none', 'header', 'body', 'all'.")
        if self.max_rows is not None and self.max_rows < 1:
            raise ValueError("max_rows must be >= 1 or None.")
        if self.min_width < 0 or self.max_width < self.min_width:
            raise ValueError("max_width must be >= min_width >= 0.")
        if self.padding < 0:
            raise ValueError("padding must be >= 0.")


@dataclass(frozen=True, slots=True)
class ScientificNotation:
    """Select when numeric cells use scientific notation.

    Examples:
        >>> ScientificNotation(scope="decimal", min_absolute=1e-4)
        ScientificNotation(scope='decimal', min_absolute=0.0001, max_absolute=1000000000000.0)
    """

    scope: Literal["none", "decimal", "integer", "all"] = "none"
    min_absolute: float = 0.0001
    max_absolute: float = 1_000_000_000_000.0

    def __post_init__(self) -> None:
        if self.scope not in {"none", "decimal", "integer", "all"}:
            raise ValueError(
                "scope must be one of: 'none', 'decimal', 'integer', 'all'."
            )
        if self.min_absolute < 0:
            raise ValueError("min_absolute must be >= 0.")
        if self.max_absolute <= self.min_absolute:
            raise ValueError("max_absolute must be greater than min_absolute.")


@dataclass(frozen=True, slots=True)
class WorksheetPart:
    """One physical worksheet produced by a logical write.

    Row and column bounds are left-closed and right-open source-table offsets.

    Examples:
        >>> WorksheetPart("Data", 0, 2, 0, 3)
        WorksheetPart(name='Data', row_start=0, row_stop=2, column_start=0, column_stop=3)
    """

    name: str
    row_start: int
    row_stop: int
    column_start: int
    column_stop: int


@dataclass(frozen=True, slots=True)
class SheetReport:
    """Result metadata for one successful :meth:`Workbook.write_sheet` call.

    Examples:
        >>> SheetReport("Data", (WorksheetPart("Data", 0, 2, 0, 1),))
        SheetReport(requested_name='Data', worksheets=(WorksheetPart(name='Data', row_start=0, row_stop=2, column_start=0, column_stop=1),), warnings=())
    """

    requested_name: str
    worksheets: tuple[WorksheetPart, ...]
    warnings: tuple[str, ...] = ()


# Private models are named for the Rust bridge contract. They aren't exported.
@dataclass(frozen=True, slots=True)
class SheetSlice:
    sheet_name: str
    row_start_inclusive: int
    row_end_exclusive: int
    col_start_inclusive: int
    col_end_exclusive: int


@dataclass(slots=True)
class XlsxReport:
    sheets: list[SheetSlice]
    warnings: list[str]


@dataclass(frozen=True, slots=True)
class _ColumnValuePlan:
    """Private source-column value contract transported to the Rust writer."""

    name: str
    kind: str
    unit: str | None = None
    timezone: str | None = None
    decimal_precision: int | None = None
    decimal_scale: int | None = None

    def to_bridge(
        self,
    ) -> tuple[
        str,
        str,
        str | None,
        str | None,
        int | None,
        int | None,
    ]:
        return (
            self.name,
            self.kind,
            self.unit,
            self.timezone,
            self.decimal_precision,
            self.decimal_scale,
        )


@dataclass(frozen=True, slots=True)
class _ValuePolicy:
    missing_value_str: str = "NA"
    nan_str: str = "NaN"
    posinf_str: str = "Inf"
    neginf_str: str = "-Inf"
    integer_coerce: Literal["coerce", "strict"] = "strict"


@dataclass(frozen=True, slots=True)
class _RowChunkPolicy:
    width_large: int = 8_000
    width_medium: int = 2_000
    size_large: int = 1_000
    size_medium: int = 2_000
    size_default: int = 10_000
    fixed_size: int | None = None


@dataclass(frozen=True, slots=True)
class _WriteOptions:
    value_policy: _ValuePolicy
    should_keep_missing_values: bool
    should_infer_numeric_cols: bool
    should_infer_integer_cols: bool
    row_chunk_policy: _RowChunkPolicy
    should_use_zip64: bool
