from __future__ import annotations

import polars as pl
import polars.selectors as cs
import pytest
from hypothesis import given
from hypothesis import strategies as st
from neatxlsx.writer import _resolve_columns, _resolve_numeric_roles

SCHEMA = pl.Schema(
    {
        "id": pl.Int64,
        "score": pl.Float64,
        "label": pl.String,
    }
)


def test_polars_selector_and_sequence_preserve_schema_order() -> None:
    assert _resolve_columns(cs.numeric(), SCHEMA, "columns") == ("id", "score")
    assert _resolve_columns([1, "id"], SCHEMA, "columns") == ("score", "id")


@pytest.mark.parametrize("value", [False, (), [], {"id"}, {"id": 1}, iter(["id"])])
def test_ambiguous_or_unordered_selectors_are_rejected(value: object) -> None:
    with pytest.raises((TypeError, ValueError)):
        _resolve_columns(value, SCHEMA, "columns")  # type: ignore[arg-type]


def test_explicit_roles_override_inference_without_disabling_other_columns() -> None:
    integer, decimal = _resolve_numeric_roles(
        SCHEMA,
        integer_columns="score",
        decimal_columns="id",
        infer_numeric=True,
        infer_integer=True,
    )

    assert integer == ("score",)
    assert decimal == ("id",)


@given(st.integers(min_value=0, max_value=2))
def test_valid_indices_round_trip_to_schema_names(index: int) -> None:
    assert _resolve_columns(index, SCHEMA, "columns") == (SCHEMA.names()[index],)
