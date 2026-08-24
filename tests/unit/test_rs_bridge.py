from __future__ import annotations

from types import SimpleNamespace

import pytest
from neatxlsx import _rs_bridge


def test_v4_native_is_rejected_by_v5_python_contract(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    native_v4 = SimpleNamespace(
        __bridge_abi__=4,
        __bridge_contract__="neatxlsx.xlsx.writer.v4",
        __bridge_transport__="arrow_c_data",
    )
    monkeypatch.setattr(_rs_bridge, "_mod_rs", native_v4)

    with pytest.raises(RuntimeError, match="python expects 5, rust exports 4"):
        _rs_bridge._validate_bridge_contract()


def test_v5_python_contract_accepts_paired_native_metadata(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    native_v5 = SimpleNamespace(
        __bridge_abi__=5,
        __bridge_contract__="neatxlsx.xlsx.writer.v5",
        __bridge_transport__="arrow_c_data",
    )
    monkeypatch.setattr(_rs_bridge, "_mod_rs", native_v5)

    _rs_bridge._validate_bridge_contract()
