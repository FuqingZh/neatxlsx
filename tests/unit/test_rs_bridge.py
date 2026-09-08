from __future__ import annotations

from types import SimpleNamespace

import pytest
from neatxlsx import _rs_bridge


def test_v5_native_is_rejected_by_v6_python_contract(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    native_v5 = SimpleNamespace(
        __bridge_abi__=5,
        __bridge_contract__="neatxlsx.xlsx.writer.v5",
        __bridge_transport__="arrow_c_data",
    )
    monkeypatch.setattr(_rs_bridge, "_mod_rs", native_v5)

    with pytest.raises(RuntimeError, match="python expects 6, rust exports 5"):
        _rs_bridge._validate_bridge_contract()


def test_v6_python_contract_accepts_paired_native_metadata(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    native_v6 = SimpleNamespace(
        __bridge_abi__=6,
        __bridge_contract__="neatxlsx.xlsx.writer.v6",
        __bridge_transport__="arrow_c_data",
    )
    monkeypatch.setattr(_rs_bridge, "_mod_rs", native_v6)

    _rs_bridge._validate_bridge_contract()


def test_v5_python_contract_is_rejected_by_v6_native(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    native_v6 = SimpleNamespace(
        __bridge_abi__=6,
        __bridge_contract__="neatxlsx.xlsx.writer.v6",
        __bridge_transport__="arrow_c_data",
    )
    monkeypatch.setattr(_rs_bridge, "EXPECTED_BRIDGE_ABI", 5)
    monkeypatch.setattr(
        _rs_bridge, "EXPECTED_BRIDGE_CONTRACT", "neatxlsx.xlsx.writer.v5"
    )
    monkeypatch.setattr(_rs_bridge, "_mod_rs", native_v6)

    with pytest.raises(RuntimeError, match="python expects 5, rust exports 6"):
        _rs_bridge._validate_bridge_contract()
