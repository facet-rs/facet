"""Type definitions for Styx parser."""

from __future__ import annotations

from dataclasses import dataclass
from enum import Enum


@dataclass(frozen=True, slots=True)
class Span:
    """A byte range in the source."""

    start: int
    end: int


class ParseError(Exception):
    """A parse error with location information."""

    def __init__(self, message: str, span: Span) -> None:
        self.message = message
        self.span = span
        super().__init__(f"parse error at {span.start}-{span.end}: {message}")


class ScalarKind(Enum):
    """The kind of scalar value."""

    BARE = "bare"
    QUOTED = "quoted"
    RAW = "raw"
    HEREDOC = "heredoc"


class PathValueKind(Enum):
    """The kind of value assigned to a path."""

    OBJECT = "object"  # Intermediate object node or explicit object
    TERMINAL = "terminal"  # Scalar, sequence, or tag-only value


@dataclass(slots=True)
class Scalar:
    """A scalar value."""

    text: str
    kind: ScalarKind
    span: Span


@dataclass(slots=True)
class Tag:
    """A tag annotation."""

    name: str
    span: Span


@dataclass(slots=True)
class Entry:
    """A key-value entry in an object."""

    key: Value
    value: Value


@dataclass(slots=True)
class Sequence:
    """A sequence of values."""

    items: list[Value]
    span: Span


@dataclass(slots=True)
class StyxObject:
    """An object with key-value entries."""

    entries: list[Entry]
    span: Span


type Payload = Scalar | Sequence | StyxObject


@dataclass(slots=True)
class Value:
    """A Styx value - can have a tag and/or payload."""

    span: Span
    tag: Tag | None = None
    payload: Payload | None = None

    def is_unit(self) -> bool:
        """Check if this is a unit value (no tag, no payload)."""
        return self.tag is None and self.payload is None


@dataclass(slots=True)
class Document:
    """A parsed Styx document."""

    entries: list[Entry]
    span: Span
