from __future__ import annotations

from typing import Any

EXPECTED_BRIDGE_ABI = 2
EXPECTED_BRIDGE_CONTRACT = "neatxlsx.xlsx.writer.v2"
EXPECTED_BRIDGE_TRANSPORT = "arrow_c_data"

_mod_rs: Any | None = None
_XlsxWriterRs: Any | None = None
_error_import: Exception | None = None
_error_contract: Exception | None = None

try:
    from . import _native as _mod_rs

    _XlsxWriterRs = _mod_rs.XlsxWriter
except Exception as exc:  # pragma: no cover
    _mod_rs = None
    _XlsxWriterRs = None
    _error_import = exc


def _validate_bridge_contract() -> None:
    if _mod_rs is None:
        return

    abi = getattr(_mod_rs, "__bridge_abi__", None)
    contract = getattr(_mod_rs, "__bridge_contract__", None)
    transport = getattr(_mod_rs, "__bridge_transport__", None)

    if abi != EXPECTED_BRIDGE_ABI:
        raise RuntimeError(
            "Rust xlsx bridge ABI mismatch: "
            f"python expects {EXPECTED_BRIDGE_ABI}, rust exports {abi!r}."
        )
    if contract != EXPECTED_BRIDGE_CONTRACT:
        raise RuntimeError(
            "Rust xlsx bridge contract mismatch: "
            f"python expects {EXPECTED_BRIDGE_CONTRACT!r}, rust exports {contract!r}."
        )
    if transport != EXPECTED_BRIDGE_TRANSPORT:
        raise RuntimeError(
            "Rust xlsx bridge transport mismatch: "
            f"python expects {EXPECTED_BRIDGE_TRANSPORT!r}, rust exports {transport!r}."
        )


if _mod_rs is not None:
    try:
        _validate_bridge_contract()
    except Exception as exc:  # pragma: no cover
        _XlsxWriterRs = None
        _error_contract = exc


def is_rs_backend_available() -> bool:
    return _XlsxWriterRs is not None


def _raise_unavailable() -> None:
    if _error_contract is not None:
        raise RuntimeError("Rust xlsx backend contract validation failed.") from _error_contract
    if _error_import is not None:
        raise RuntimeError("Rust xlsx backend import failed.") from _error_import
    raise RuntimeError("Rust xlsx backend is unavailable")


def create_xlsx_writer_via_rs(
    file_out: str,
    *,
    fmt_text: Any = None,
    fmt_integer: Any = None,
    fmt_decimal: Any = None,
    fmt_scientific: Any = None,
    fmt_header: Any = None,
    options_write: Any = None,
):
    if _XlsxWriterRs is None:  # pragma: no cover
        _raise_unavailable()
    else:
        return _XlsxWriterRs(
            file_out,
            fmt_text=fmt_text,
            fmt_integer=fmt_integer,
            fmt_decimal=fmt_decimal,
            fmt_scientific=fmt_scientific,
            fmt_header=fmt_header,
            options_write=options_write,
        )
