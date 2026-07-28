"""Private defaults shared by the Python facade."""

from collections.abc import Mapping
from types import MappingProxyType
from typing import Literal, TypeAlias

from .spec import Format

N_NROWS_EXCEL_MAX = 1_048_576
N_NCOLS_EXCEL_MAX = 16_384
N_LEN_EXCEL_SHEET_NAME_MAX = 31

FormatRole = Literal["text", "integer", "decimal", "scientific", "header"]
_BASE_FORMAT = Format(
    font_name="Times New Roman",
    font_size=11,
    border=1,
    align="left",
    valign="vcenter",
)
DEFAULT_FORMATS: Mapping[FormatRole, Format] = MappingProxyType(
    {
        "text": _BASE_FORMAT,
        "header": _BASE_FORMAT.replace(bold=True, align="center"),
        "integer": _BASE_FORMAT.replace(num_format="0"),
        "decimal": _BASE_FORMAT.replace(num_format="0.0000"),
        "scientific": _BASE_FORMAT.replace(num_format="0.00E+0"),
    }
)

ColumnIdentifier: TypeAlias = str | int
