from __future__ import annotations

from collections.abc import Sequence
from typing import Any, Literal

from .spec import AutofitPolicy, ScientificPolicy, XlsxReport

__bridge_abi__: int
__bridge_contract__: str
__bridge_transport__: str

class XlsxArrowDrainProfile:
    batches: int
    rows: int
    cols: int
    cells: int

class XlsxWriter:
    file_out: str

    def __init__(
        self,
        file_out: str,
        *,
        fmt_text: Any = ...,
        fmt_integer: Any = ...,
        fmt_decimal: Any = ...,
        fmt_scientific: Any = ...,
        fmt_header: Any = ...,
        options_write: Any = ...,
    ) -> None: ...
    def __enter__(self) -> XlsxWriter: ...
    def __exit__(
        self,
        exc_type: type | None,
        exc: BaseException | None,
        tb: Any | None,
    ) -> None: ...
    def close(self) -> None: ...
    def report(self) -> tuple[XlsxReport, ...]: ...
    def write_sheet(
        self,
        body: Any,
        sheet_name: str,
        *,
        header: Any | None = ...,
        cols_integer: Sequence[str | int] | str | int | None = ...,
        cols_decimal: Sequence[str | int] | str | int | Literal[False] | None = ...,
        num_frozen_cols: int = ...,
        num_frozen_rows: int | None = ...,
        should_merge_header: bool = ...,
        should_keep_missing_values: bool | None = ...,
        policy_autofit: AutofitPolicy | None = ...,
        policy_scientific: ScientificPolicy | None = ...,
    ) -> XlsxWriter: ...
    def write_sheet_batches(
        self,
        batches_scan: Any,
        batches_write: Any,
        sheet_name: str,
        *,
        header: Any | None = ...,
        cols_integer: Sequence[str | int] | str | int | None = ...,
        cols_decimal: Sequence[str | int] | str | int | Literal[False] | None = ...,
        num_frozen_cols: int = ...,
        num_frozen_rows: int | None = ...,
        should_merge_header: bool = ...,
        should_keep_missing_values: bool | None = ...,
        policy_autofit: AutofitPolicy | None = ...,
        policy_scientific: ScientificPolicy | None = ...,
        schema_body: Any | None = ...,
    ) -> XlsxWriter: ...
    def write_sheet_batches_single_pass(
        self,
        batches_write: Any,
        sheet_name: str,
        *,
        header: Any | None = ...,
        cols_integer: Sequence[str | int] | str | int | None = ...,
        cols_decimal: Sequence[str | int] | str | int | Literal[False] | None = ...,
        num_frozen_cols: int = ...,
        num_frozen_rows: int | None = ...,
        should_merge_header: bool = ...,
        should_keep_missing_values: bool | None = ...,
        policy_autofit: AutofitPolicy | None = ...,
        policy_scientific: ScientificPolicy | None = ...,
        schema_body: Any | None = ...,
    ) -> XlsxWriter: ...

def _profile_arrow_drain(source: Any) -> XlsxArrowDrainProfile: ...
