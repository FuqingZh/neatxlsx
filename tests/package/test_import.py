from __future__ import annotations

from importlib.metadata import version

import neatxlsx as nx


def test_installed_distribution_and_extension_are_importable() -> None:
    assert version("neatxlsx") == "0.1.0"
    assert nx.Workbook.__module__ == "neatxlsx.writer"
