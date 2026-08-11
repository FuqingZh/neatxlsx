from datetime import UTC, datetime

import polars as pl
import pytest
from neatxlsx._dtype import prepare_lazy_frame


def test_dtype_preflight_keeps_schema_order_and_lazy_normalization() -> None:
    lazy = pl.DataFrame(
        {
            "category": pl.Series(["a"], dtype=pl.Categorical),
            "aware": pl.Series(
                [datetime(2024, 1, 1, tzinfo=UTC)],
                dtype=pl.Datetime("us", "UTC"),
            ),
            "value": [1],
        }
    ).lazy()

    preparation = prepare_lazy_frame(lazy, lazy.collect_schema())

    assert [plan.name for plan in preparation.plans] == ["category", "aware", "value"]
    assert [plan.kind for plan in preparation.plans] == ["string", "string", "integer"]
    assert preparation.lazy.collect_schema() == pl.Schema(
        {"category": pl.String, "aware": pl.String, "value": pl.Int64}
    )


@pytest.mark.parametrize(
    ("dtype", "values"),
    [
        (pl.Binary, [b"x"]),
        (pl.List(pl.Int64), [[1]]),
        (pl.Array(pl.Int64, 2), [[1, 2]]),
        (pl.Struct({"x": pl.Int64}), [{"x": 1}]),
    ],
)
def test_dtype_preflight_rejects_opaque_nested_values(
    dtype: pl.DataType, values: list[object]
) -> None:
    lazy = pl.DataFrame({"payload": pl.Series("payload", values, dtype=dtype)}).lazy()

    with pytest.raises(TypeError, match="Convert the column to String explicitly"):
        prepare_lazy_frame(lazy, lazy.collect_schema())
