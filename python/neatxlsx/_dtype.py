"""Private Polars dtype admission and Arrow bridge planning."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, cast

import polars as pl

from .spec import _ColumnValuePlan

_SUPPORTED_BASE_TYPES = {
    pl.Null,
    pl.Boolean,
    pl.Int8,
    pl.Int16,
    pl.Int32,
    pl.Int64,
    pl.Int128,
    pl.UInt8,
    pl.UInt16,
    pl.UInt32,
    pl.UInt64,
    pl.Float32,
    pl.Float64,
    pl.Decimal,
    pl.String,
    pl.Categorical,
    pl.Enum,
    pl.Date,
    pl.Datetime,
    pl.Time,
    pl.Duration,
}


@dataclass(frozen=True, slots=True)
class _DtypePreparation:
    lazy: pl.LazyFrame
    plans: tuple[_ColumnValuePlan, ...]


def prepare_lazy_frame(lazy: pl.LazyFrame, schema: pl.Schema) -> _DtypePreparation:
    """Admit a schema and lazily normalize labels/timezone-aware datetimes.

    The function never collects rows.  The returned plans retain the source
    order even when one of the admitted logical dtypes is normalized to text.
    """

    expressions: list[pl.Expr] = []
    plans: list[_ColumnValuePlan] = []
    for name, dtype in schema.items():
        base_type = dtype.base_type()
        if base_type not in _SUPPORTED_BASE_TYPES:
            raise TypeError(
                f"Unsupported Polars dtype for column {name!r}: {dtype}. "
                "Convert the column to String explicitly before write_sheet()."
            )

        if base_type is pl.Categorical or base_type is pl.Enum:
            expressions.append(pl.col(name).cast(pl.String).alias(name))
            plans.append(_ColumnValuePlan(name=name, kind="string"))
            continue

        if base_type is pl.Datetime:
            dtype_any = cast(Any, dtype)
            unit = str(dtype_any.time_unit)
            timezone = dtype_any.time_zone
            if timezone is not None:
                # Polars' string cast preserves the numeric offset and source
                # precision without materializing Python datetime objects.
                expressions.append(pl.col(name).cast(pl.String).alias(name))
                plans.append(
                    _ColumnValuePlan(
                        name=name,
                        kind="string",
                        unit=unit,
                        timezone=str(timezone),
                    )
                )
            else:
                plans.append(_ColumnValuePlan(name=name, kind="datetime", unit=unit))
            continue

        if base_type is pl.Decimal:
            dtype_any = cast(Any, dtype)
            plans.append(
                _ColumnValuePlan(
                    name=name,
                    kind="decimal",
                    decimal_precision=int(dtype_any.precision or 0),
                    decimal_scale=int(dtype_any.scale or 0),
                )
            )
            continue

        if base_type is pl.Date:
            plans.append(_ColumnValuePlan(name=name, kind="date", unit="day"))
            continue
        if base_type is pl.Time:
            plans.append(_ColumnValuePlan(name=name, kind="time", unit="ns"))
            continue
        if base_type is pl.Duration:
            dtype_any = cast(Any, dtype)
            plans.append(
                _ColumnValuePlan(
                    name=name, kind="duration", unit=str(dtype_any.time_unit)
                )
            )
            continue
        if base_type is pl.Boolean:
            plans.append(_ColumnValuePlan(name=name, kind="boolean"))
            continue
        if base_type in {
            pl.Int8,
            pl.Int16,
            pl.Int32,
            pl.Int64,
            pl.Int128,
            pl.UInt8,
            pl.UInt16,
            pl.UInt32,
            pl.UInt64,
        }:
            plans.append(_ColumnValuePlan(name=name, kind="integer"))
            continue
        if base_type in {pl.Float32, pl.Float64}:
            plans.append(_ColumnValuePlan(name=name, kind="float"))
            continue
        if base_type is pl.Null:
            plans.append(_ColumnValuePlan(name=name, kind="null"))
            continue
        # String is the only remaining admitted base type.
        plans.append(_ColumnValuePlan(name=name, kind="string"))

    normalized = lazy.with_columns(expressions) if expressions else lazy
    return _DtypePreparation(normalized, tuple(plans))
