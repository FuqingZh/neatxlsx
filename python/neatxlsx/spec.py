from dataclasses import dataclass, field, replace
from typing import Any, Literal, Self, cast


class _MissingType:
    __slots__ = ()


_FORMAT_UNSET = _MissingType()


################################################################################
# #region CellFormatSpecification
@dataclass(frozen=True, slots=True)
class CellFormatPatch:
    # 字段名严格对齐 XlsxWriter format properties keys
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

    def with_(
        self,
        *,
        font_name: str | None = cast(Any, _FORMAT_UNSET),
        font_size: int | None = cast(Any, _FORMAT_UNSET),
        bold: bool | None = cast(Any, _FORMAT_UNSET),
        italic: bool | None = cast(Any, _FORMAT_UNSET),
        align: str | None = cast(Any, _FORMAT_UNSET),
        valign: str | None = cast(Any, _FORMAT_UNSET),
        border: int | None = cast(Any, _FORMAT_UNSET),
        text_wrap: bool | None = cast(Any, _FORMAT_UNSET),
        top: int | None = cast(Any, _FORMAT_UNSET),
        bottom: int | None = cast(Any, _FORMAT_UNSET),
        left: int | None = cast(Any, _FORMAT_UNSET),
        right: int | None = cast(Any, _FORMAT_UNSET),
        num_format: str | None = cast(Any, _FORMAT_UNSET),
        bg_color: str | None = cast(Any, _FORMAT_UNSET),
        font_color: str | None = cast(Any, _FORMAT_UNSET),
    ) -> Self:
        data: dict[str, Any] = {}
        if font_name is not _FORMAT_UNSET:
            data["font_name"] = font_name
        if font_size is not _FORMAT_UNSET:
            data["font_size"] = font_size
        if bold is not _FORMAT_UNSET:
            data["bold"] = bold
        if italic is not _FORMAT_UNSET:
            data["italic"] = italic
        if align is not _FORMAT_UNSET:
            data["align"] = align
        if valign is not _FORMAT_UNSET:
            data["valign"] = valign
        if border is not _FORMAT_UNSET:
            data["border"] = border
        if text_wrap is not _FORMAT_UNSET:
            data["text_wrap"] = text_wrap
        if top is not _FORMAT_UNSET:
            data["top"] = top
        if bottom is not _FORMAT_UNSET:
            data["bottom"] = bottom
        if left is not _FORMAT_UNSET:
            data["left"] = left
        if right is not _FORMAT_UNSET:
            data["right"] = right
        if num_format is not _FORMAT_UNSET:
            data["num_format"] = num_format
        if bg_color is not _FORMAT_UNSET:
            data["bg_color"] = bg_color
        if font_color is not _FORMAT_UNSET:
            data["font_color"] = font_color
        return replace(self, **data)

    def merge(self, other: "CellFormatPatch") -> "CellFormatPatch":
        # 右侧非 None 覆盖左侧
        data = {
            _k: (
                getattr(other, _k)
                if getattr(other, _k) is not None
                else getattr(self, _k)
            )
            for _k in self.__dataclass_fields__
        }
        return CellFormatPatch(**data)

    def to_xlsxwriter(self) -> dict[str, Any]:
        return {
            _k: getattr(self, _k)
            for _k in self.__dataclass_fields__
            if getattr(self, _k) is not None
        }


@dataclass(frozen=True, slots=True)
class CellBorder:
    top: int
    bottom: int
    left: int
    right: int


# #endregion
################################################################################
# #region WriteOptions


@dataclass(frozen=True, slots=True)
class XlsxValuePolicy:
    missing_value_str: str = "NA"
    nan_str: str = "NaN"
    posinf_str: str = "Inf"
    neginf_str: str = "-Inf"
    integer_coerce: Literal["coerce", "strict"] = "strict"


@dataclass(frozen=True, slots=True)
class XlsxRowChunkPolicy:
    width_large: int = 8_000
    width_medium: int = 2_000
    size_large: int = 1_000
    size_medium: int = 2_000
    size_default: int = 10_000
    fixed_size: int | None = None


@dataclass(frozen=True, slots=True)
class XlsxWriteOptions:
    value_policy: XlsxValuePolicy = field(default_factory=XlsxValuePolicy)
    should_keep_missing_values: bool = False
    should_infer_numeric_cols: bool = True
    should_infer_integer_cols: bool = True
    row_chunk_policy: XlsxRowChunkPolicy = field(
        default_factory=XlsxRowChunkPolicy
    )
    base_format_patch: "CellFormatPatch" = field(
        default_factory=lambda: CellFormatPatch(
            border=0, top=0, bottom=0, left=0, right=0
        )
    )
    should_use_zip64: bool = True


@dataclass(frozen=True, slots=True)
class ScientificPolicy:
    scope: Literal["none", "decimal", "integer", "all"] = "none"
    thr_min: float = 0.0001
    thr_max: float = 1_000_000_000_000.0


@dataclass(frozen=True, slots=True)
class AutofitPolicy:
    mode: Literal["none", "header", "body", "all"] = "header"
    height_body_inferred_max: int | None = 20_000
    width_cell_min: int = 8
    width_cell_max: int = 60
    width_cell_padding: int = 2


# #endregion
################################################################################
# #region SheetFormatSpecification
@dataclass(frozen=True, slots=True)
class SheetSlice:
    sheet_name: str
    row_start_inclusive: int
    row_end_exclusive: int  # exclusive in source df rows
    col_start_inclusive: int
    col_end_exclusive: int  # exclusive in source df cols


@dataclass(frozen=True, slots=True)
class SheetHorizontalMerge:
    row_idx_start: int
    col_idx_start: int
    col_idx_end: int  # inclusive
    text: str


# #endregion
################################################################################
# #region ReportSpecification
@dataclass(slots=True)
class XlsxReport:
    sheets: list[SheetSlice]
    warnings: list[str]

    def warn(self, msg: str) -> None:
        self.warnings.append(str(msg))


# #endregion
################################################################################
