from __future__ import annotations

from collections.abc import Callable
from typing import Any, cast

import neatxlsx as nx
import pytest


def test_format_replace_distinguishes_false_and_zero_from_unspecified() -> None:
    original = nx.Format(bold=True, border=1)

    changed = original.replace(bold=False, border=0)

    assert changed.bold is False
    assert changed.border == 0
    assert original.bold is True
    assert original.border == 1


@pytest.mark.parametrize(
    ("factory", "message"),
    [
        (lambda: nx.Autofit(mode=cast(Any, "invalid")), "mode must be one of"),
        (lambda: nx.Autofit(max_rows=0), "max_rows"),
        (lambda: nx.Autofit(min_width=10, max_width=5), "max_width"),
        (
            lambda: nx.ScientificNotation(scope=cast(Any, "invalid")),
            "scope must be one of",
        ),
        (
            lambda: nx.ScientificNotation(min_absolute=2, max_absolute=1),
            "max_absolute",
        ),
    ],
)
def test_policy_types_fail_fast(factory: Callable[[], object], message: str) -> None:
    with pytest.raises(ValueError, match=message):
        factory()
