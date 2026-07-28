"""Public neatxlsx exception hierarchy."""


class Error(Exception):
    """Base class for neatxlsx backend and lifecycle failures.

    Examples:
        >>> isinstance(WriteError("write failed"), Error)
        True
    """


class StateError(Error):
    """Raised when an operation conflicts with the workbook lifecycle.

    Examples:
        >>> isinstance(StateError("closed"), Error)
        True
    """


class WriteError(Error):
    """Raised when XLSX generation fails after input normalization.

    Examples:
        >>> isinstance(WriteError("backend failed"), Error)
        True
    """


class CommitError(Error):
    """Raised when a completed temporary workbook cannot replace its target.

    The same workbook may retry :meth:`~neatxlsx.Workbook.close`, or callers
    may discard it with :meth:`~neatxlsx.Workbook.abort`.

    Examples:
        >>> error = CommitError("report.xlsx", "permission denied")
        >>> error.target
        'report.xlsx'
    """

    def __init__(self, target: str, message: str) -> None:
        super().__init__(f"Could not commit workbook to {target!r}: {message}")
        self.target = target
        self.retryable = True
