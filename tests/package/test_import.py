from __future__ import annotations

from importlib.metadata import version

import neatxlsx as nx
from neatxlsx import _native


def test_installed_distribution_and_extension_are_importable() -> None:
    assert version("neatxlsx") == "0.1.0"
    assert nx.Workbook.__module__ == "neatxlsx.writer"
    assert _native.__build_profile__
    assert _native.__bridge_abi__ == 4
    assert _native.__bridge_contract__ == "neatxlsx.xlsx.writer.v4"
    assert _native.__bridge_transport__ == "arrow_c_data"
