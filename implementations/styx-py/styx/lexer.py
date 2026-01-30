"""Lexer for Styx configuration language."""

from __future__ import annotations

import re
from dataclasses import dataclass
from enum import Enum
from typing import Final

from .types import ParseError, Span


class TokenType(Enum):
    """Token types."""

    SCALAR = "scalar"
    QUOTED = "quoted"
    RAW = "raw"
    HEREDOC = "heredoc"
    LBRACE = "lbrace"
    RBRACE = "rbrace"
    LPAREN = "lparen"
    RPAREN = "rparen"
    COMMA = "comma"
    AT = "at"
    TAG = "tag"
    GT = "gt"
    EOF = "eof"


@dataclass(slots=True)
class Token:
    """A lexer token."""

    type: TokenType
    text: str
    span: Span
    had_whitespace_before: bool
    had_newline_before: bool


SPECIAL_CHARS: Final[frozenset[str]] = frozenset(
    ["{", "}", "(", ")", ",", '"', ">", " ", "\t", "\n", "\r"]
)

TAG_START_RE: Final[re.Pattern[str]] = re.compile(r"[A-Za-z_]")
TAG_CHAR_RE: Final[re.Pattern[str]] = re.compile(r"[A-Za-z0-9_\-]")


class Lexer:
    """Tokenizer for Styx source code."""

    __slots__ = ("byte_pos", "pos", "source")

    def __init__(self, source: str) -> None:
        self.source = source
        self.pos = 0  # character position
        self.byte_pos = 0  # byte position for spans

    def _peek(self, offset: int = 0) -> str:
        """Look ahead in the source."""
        idx = self.pos + offset
        if idx >= len(self.source):
            return ""
        return self.source[idx]

    def _advance(self) -> str:
        """Consume and return the next character."""
        if self.pos >= len(self.source):
            return ""
        ch = self.source[self.pos]
        self.pos += 1
        self.byte_pos += len(ch.encode("utf-8"))
        return ch

    def _skip_whitespace_and_comments(self) -> tuple[bool, bool]:
        """Skip whitespace and comments, return (had_whitespace, had_newline)."""
        had_whitespace = False
        had_newline = False

        while self.pos < len(self.source):
            ch = self._peek()
            if ch in (" ", "\t", "\r"):
                had_whitespace = True
                self._advance()
            elif ch == "\n":
                had_whitespace = True
                had_newline = True
                self._advance()
            elif ch == "/" and self._peek(1) == "/":
                had_whitespace = True
                while self.pos < len(self.source) and self._peek() != "\n":
                    self._advance()
            else:
                break

        return had_whitespace, had_newline

    def _is_tag_start(self, ch: str) -> bool:
        """Check if character can start a tag name."""
        return bool(TAG_START_RE.match(ch))

    def _is_tag_char(self, ch: str) -> bool:
        """Check if character can be in a tag name."""
        return bool(TAG_CHAR_RE.match(ch))

    def next_token(self) -> Token:
        """Return the next token."""
        had_whitespace, had_newline = self._skip_whitespace_and_comments()

        if self.pos >= len(self.source):
            return Token(
                type=TokenType.EOF,
                text="",
                span=Span(self.byte_pos, self.byte_pos),
                had_whitespace_before=had_whitespace,
                had_newline_before=had_newline,
            )

        start = self.byte_pos
        ch = self._peek()

        # Single-character tokens
        match ch:
            case "{":
                self._advance()
                return Token(
                    TokenType.LBRACE, "{", Span(start, self.byte_pos), had_whitespace, had_newline
                )
            case "}":
                self._advance()
                return Token(
                    TokenType.RBRACE, "}", Span(start, self.byte_pos), had_whitespace, had_newline
                )
            case "(":
                self._advance()
                return Token(
                    TokenType.LPAREN, "(", Span(start, self.byte_pos), had_whitespace, had_newline
                )
            case ")":
                self._advance()
                return Token(
                    TokenType.RPAREN, ")", Span(start, self.byte_pos), had_whitespace, had_newline
                )
            case ",":
                self._advance()
                return Token(
                    TokenType.COMMA, ",", Span(start, self.byte_pos), had_whitespace, had_newline
                )
            case ">":
                self._advance()
                return Token(
                    TokenType.GT, ">", Span(start, self.byte_pos), had_whitespace, had_newline
                )

        # @ - either unit or tag
        if ch == "@":
            self._advance()
            if self._is_tag_start(self._peek()):
                name_start = self.pos
                while self._is_tag_char(self._peek()):
                    self._advance()
                name = self.source[name_start : self.pos]
                return Token(
                    TokenType.TAG, name, Span(start, self.byte_pos), had_whitespace, had_newline
                )
            return Token(TokenType.AT, "@", Span(start, self.byte_pos), had_whitespace, had_newline)

        # Quoted string
        if ch == '"':
            return self._read_quoted_string(start, had_whitespace, had_newline)

        # Raw string
        if ch == "r" and self._peek(1) in ('"', "#"):
            return self._read_raw_string(start, had_whitespace, had_newline)

        # Heredoc - only if << is followed by uppercase letter
        if ch == "<" and self._peek(1) == "<":
            after_lt_lt = self._peek(2)
            if after_lt_lt.isupper():
                return self._read_heredoc(start, had_whitespace, had_newline)
            # << not followed by uppercase - return error at just <<
            self._advance()  # <
            self._advance()  # <
            error_end = self.byte_pos
            # Skip rest of line for recovery
            while self.pos < len(self.source) and self._peek() != "\n":
                self._advance()
            raise ParseError("unexpected token", Span(start, error_end))

        # Bare scalar
        return self._read_bare_scalar(start, had_whitespace, had_newline)

    def _read_quoted_string(self, start: int, had_whitespace: bool, had_newline: bool) -> Token:
        """Read a quoted string."""
        self._advance()  # opening "
        text = ""

        while self.pos < len(self.source):
            ch = self._peek()
            if ch == '"':
                self._advance()
                return Token(
                    TokenType.QUOTED, text, Span(start, self.byte_pos), had_whitespace, had_newline
                )
            if ch == "\\":
                escape_start = self.byte_pos
                self._advance()
                escaped = self._advance()
                match escaped:
                    case "n":
                        text += "\n"
                    case "r":
                        text += "\r"
                    case "t":
                        text += "\t"
                    case "\\":
                        text += "\\"
                    case '"':
                        text += '"'
                    case "u":
                        text += self._read_unicode_escape()
                    case _:
                        raise ParseError(
                            f"invalid escape sequence: \\{escaped}",
                            Span(escape_start, self.byte_pos),
                        )
            elif ch in ("\n", "\r"):
                # Unterminated string - include the newline in the span
                self._advance()
                if ch == "\r" and self._peek() == "\n":
                    self._advance()
                raise ParseError("unexpected token", Span(start, self.byte_pos))
            else:
                text += self._advance()

        # EOF without closing quote - error
        raise ParseError("unexpected token", Span(start, self.byte_pos))

    def _read_unicode_escape(self) -> str:
        """Read a unicode escape sequence."""
        if self._peek() == "{":
            self._advance()
            hex_str = ""
            while self._peek() != "}" and self.pos < len(self.source):
                hex_str += self._advance()
            self._advance()
            return chr(int(hex_str, 16))
        else:
            hex_str = ""
            for _ in range(4):
                hex_str += self._advance()
            return chr(int(hex_str, 16))

    def _read_raw_string(self, start: int, had_whitespace: bool, had_newline: bool) -> Token:
        """Read a raw string."""
        self._advance()  # r
        hashes = 0
        while self._peek() == "#":
            self._advance()
            hashes += 1
        self._advance()  # opening "

        text = ""
        close_pattern = '"' + "#" * hashes

        while self.pos < len(self.source):
            if self.source[self.pos : self.pos + len(close_pattern)] == close_pattern:
                for _ in range(len(close_pattern)):
                    self._advance()
                return Token(
                    TokenType.RAW, text, Span(start, self.byte_pos), had_whitespace, had_newline
                )
            text += self._advance()

        raise ParseError("unclosed raw string", Span(start, self.byte_pos))

    def _read_heredoc(self, start: int, had_whitespace: bool, had_newline: bool) -> Token:
        """Read a heredoc."""
        self._advance()  # <
        self._advance()  # <

        delimiter = ""
        while self.pos < len(self.source) and self._peek() != "\n":
            delimiter += self._advance()
        if self.pos < len(self.source):
            self._advance()  # newline

        # Track content start (after the opening line)
        content_start = self.byte_pos

        text = ""
        bare_delimiter = delimiter.split(",")[0]

        while self.pos < len(self.source):
            line = ""
            while self.pos < len(self.source) and self._peek() != "\n":
                line += self._advance()

            # Check for exact match (no indentation)
            if line == bare_delimiter:
                return Token(
                    TokenType.HEREDOC, text, Span(start, self.byte_pos), had_whitespace, had_newline
                )

            # Check for indented closing delimiter
            stripped = line.lstrip(" \t")
            if stripped == bare_delimiter:
                indent_len = len(line) - len(stripped)
                # Dedent the content by stripping up to indent_len from each line
                result = self._dedent_heredoc(text, indent_len)
                return Token(
                    TokenType.HEREDOC,
                    result,
                    Span(start, self.byte_pos),
                    had_whitespace,
                    had_newline,
                )

            text += line
            if self.pos < len(self.source) and self._peek() == "\n":
                self._advance()
                text += "\n"

        # EOF without closing delimiter - error points at the unmatched content
        raise ParseError("unexpected token", Span(content_start, self.byte_pos))

    def _dedent_heredoc(self, content: str, indent_len: int) -> str:
        """Strip up to indent_len whitespace characters from the start of each line."""
        lines = content.split("\n")
        result = []
        for line in lines:
            stripped = 0
            for ch in line:
                if stripped >= indent_len:
                    break
                if ch in (" ", "\t"):
                    stripped += 1
                else:
                    break
            result.append(line[stripped:])
        return "\n".join(result)

    def _read_bare_scalar(self, start: int, had_whitespace: bool, had_newline: bool) -> Token:
        """Read a bare scalar."""
        text = ""
        while self.pos < len(self.source):
            ch = self._peek()
            if ch in SPECIAL_CHARS:
                break
            text += self._advance()
        return Token(
            TokenType.SCALAR, text, Span(start, self.byte_pos), had_whitespace, had_newline
        )
