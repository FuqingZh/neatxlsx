from __future__ import annotations

from types import SimpleNamespace

import pytest
from neatxlsx import _rs_bridge


def test_v3_native_is_rejected_by_v4_python_contract(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    native_v3 = SimpleNamespace(
        __bridge_abi__=3,
        __bridge_contract__="neatxlsx.xlsx.writer.v3",
        __bridge_transport__="arrow_c_data",
    )
    monkeypatch.setattr(_rs_bridge, "_mod_rs", native_v3)

    with pytest.raises(RuntimeError, match="python expects 4, rust exports 3"):
        _rs_bridge._validate_bridge_contract()
