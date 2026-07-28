from collections.abc import Mapping
from types import MappingProxyType
from typing import Literal, TypeAlias

from .spec import CellFormatPatch, XlsxWriteOptions

N_NROWS_EXCEL_MAX = 1_048_576
N_NCOLS_EXCEL_MAX = 16_384
N_LEN_EXCEL_SHEET_NAME_MAX = 31
TUP_EXCEL_ILLEGAL = ("*", ":", "?", "/", "\\", "[", "]")

# Strategy/Preference/Adjustable Parameters for XLSX I/O operations.

LIT_FMT_KEYS = Literal["text", "integer", "decimal", "scientific", "header"]
_cls_base_fmt_patch = CellFormatPatch(
    font_name="Times New Roman", font_size=11, border=1, align="left", valign="vcenter"
)

DEFAULT_XLSX_FORMATS: Mapping[LIT_FMT_KEYS, CellFormatPatch] = MappingProxyType(
    {
        "text": _cls_base_fmt_patch,
        "header": _cls_base_fmt_patch.with_(bold=True, align="center"),
        "integer": _cls_base_fmt_patch.with_(num_format="0"),
        "decimal": _cls_base_fmt_patch.with_(num_format="0.0000"),
        "scientific": _cls_base_fmt_patch.with_(num_format="0.00E+0"),
    }
)

DEFAULT_XLSX_WRITE_OPTIONS = XlsxWriteOptions()

# `str` means a literal column name; `int` means a zero-based column index.
ColumnIdentifier: TypeAlias = str | int
