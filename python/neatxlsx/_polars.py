"""The single compatibility boundary for Polars' unstable batch API."""

from __future__ import annotations

from typing import Any

import polars as pl


def collect_batches(frame: pl.LazyFrame, *, chunk_size: int) -> Any:
    """Return Polars streaming batches using the supported package contract."""
    method = getattr(frame, "collect_batches", None)
    if not callable(method):
        raise RuntimeError(
            "neatxlsx requires polars>=1.38.1 with LazyFrame.collect_batches()."
        )
    return method(chunk_size=chunk_size)
