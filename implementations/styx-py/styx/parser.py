"""Parser for Styx configuration language."""

from __future__ import annotations

from .lexer import Lexer, Token, TokenType
from .types import (
    Document,
    Entry,
    ParseError,
    Scalar,
    ScalarKind,
    Separator,
    Sequence,
    Span,
    StyxObject,
    Tag,
    Value,
)


class Parser:
    """Parser for Styx documents."""

    __slots__ = ("current", "lexer", "peeked")

    def __init__(self, source: str) -> None:
        self.lexer = Lexer(source)
        self.current = self.lexer.next_token()
        self.peeked: Token | None = None

    def _advance(self) -> Token:
        """Consume and return the current token."""
        prev = self.current
        if self.peeked:
            self.current = self.peeked
            self.peeked = None
        else:
            self.current = self.lexer.next_token()
        return prev

    def _peek(self) -> Token:
        """Look ahead one token."""
        if not self.peeked:
            self.peeked = self.lexer.next_token()
        return self.peeked

    def _check(self, *types: TokenType) -> bool:
        """Check if current token matches any of the given types."""
        return self.current.type in types

    def _expect(self, token_type: TokenType) -> Token:
        """Expect a specific token type."""
        if self.current.type != token_type:
            raise ParseError(
                f"expected {token_type.value}, got {self.current.type.value}",
                self.current.span,
            )
        return self._advance()

    def parse(self) -> Document:
        """Parse a complete document."""
        entries: list[Entry] = []
        start = self.current.span.start
        seen_keys: dict[str, Span] = {}

        while not self._check(TokenType.EOF):
            entry = self._parse_entry_with_dup_check(seen_keys)
            if entry:
                entries.append(entry)

        return Document(
            entries=entries,
            span=Span(start, self.current.span.end),
        )

    def _parse_entry_with_dup_check(self, seen_keys: dict[str, Span]) -> Entry | None:
        """Parse an entry with duplicate key checking."""
        while self._check(TokenType.COMMA):
            self._advance()

        if self._check(TokenType.EOF, TokenType.RBRACE):
            return None

        key = self._parse_value()

        # Special case: object in key position gets implicit unit key
        if key.payload is not None and isinstance(key.payload, StyxObject):
            if not self.current.had_newline_before and not self._check(
                TokenType.EOF, TokenType.RBRACE, TokenType.COMMA
            ):
                self._parse_value()  # Drop trailing value
            unit_key = Value(span=Span(-1, -1))
            return Entry(key=unit_key, value=key)

        # Check for dotted path in bare scalar key
        if (
            key.payload is not None
            and isinstance(key.payload, Scalar)
            and key.payload.kind == ScalarKind.BARE
        ):
            text = key.payload.text
            if "." in text:
                return self._expand_dotted_path(text, key.span, seen_keys)

        # Check for duplicate key
        key_text = self._get_key_text(key)
        if key_text is not None:
            if key_text in seen_keys:
                raise ParseError("duplicate key", key.span)
            seen_keys[key_text] = key.span

        self._validate_key(key)

        # Check for implicit unit
        if self.current.had_newline_before or self._check(TokenType.EOF, TokenType.RBRACE):
            return Entry(key=key, value=Value(span=key.span))

        value = self._parse_value()
        return Entry(key=key, value=value)

    def _get_key_text(self, key: Value) -> str | None:
        """Get the text representation of a key for duplicate checking."""
        if key.payload is not None and isinstance(key.payload, Scalar):
            return key.payload.text
        if key.tag is not None and key.payload is None:
            return f"@{key.tag.name}"
        return None

    def _validate_key(self, key: Value) -> None:
        """Validate that a value can be used as a key."""
        if key.payload is not None:
            if isinstance(key.payload, Sequence):
                raise ParseError("invalid key", key.span)
            if isinstance(key.payload, Scalar) and key.payload.kind == ScalarKind.HEREDOC:
                raise ParseError("invalid key", key.span)

    def _expand_dotted_path(self, path_text: str, span: Span, seen_keys: dict[str, Span]) -> Entry:
        """Expand a dotted path into nested objects."""
        segments = path_text.split(".")

        if any(s == "" for s in segments):
            raise ParseError("invalid key", span)

        first_segment = segments[0]
        if first_segment in seen_keys:
            raise ParseError("duplicate key", span)
        seen_keys[first_segment] = span

        segment_spans: list[Span] = []
        offset = span.start
        for segment in segments:
            segment_bytes = len(segment.encode("utf-8"))
            segment_spans.append(Span(offset, offset + segment_bytes))
            offset += segment_bytes + 1

        value = self._parse_value()

        result = value
        for i in range(len(segments) - 1, 0, -1):
            seg_span = segment_spans[i]
            segment_key = Value(
                span=seg_span,
                payload=Scalar(text=segments[i], kind=ScalarKind.BARE, span=seg_span),
            )
            result = Value(
                span=span,
                payload=StyxObject(
                    entries=[Entry(key=segment_key, value=result)],
                    separator=Separator.NEWLINE,
                    span=span,
                ),
            )

        first_span = segment_spans[0]
        outer_key = Value(
            span=first_span,
            payload=Scalar(text=first_segment, kind=ScalarKind.BARE, span=first_span),
        )

        return Entry(key=outer_key, value=result)

    def _parse_attribute_value(self) -> Value:
        """Parse a value in attribute context."""
        if self._check(TokenType.LBRACE):
            obj = self._parse_object()
            return Value(span=obj.span, payload=obj)
        if self._check(TokenType.LPAREN):
            seq = self._parse_sequence()
            return Value(span=seq.span, payload=seq)
        if self._check(TokenType.TAG):
            return self._parse_tag_value()
        if self._check(TokenType.AT):
            at_token = self._advance()
            return Value(span=at_token.span)
        scalar = self._parse_scalar()
        return Value(span=scalar.span, payload=scalar)

    def _parse_tag_value(self) -> Value:
        """Parse a tag with optional payload."""
        start = self.current.span.start
        tag_token = self._advance()
        tag = Tag(name=tag_token.text, span=tag_token.span)

        if not self.current.had_whitespace_before:
            if self._check(TokenType.LBRACE):
                obj = self._parse_object()
                return Value(span=obj.span, tag=tag, payload=obj)
            if self._check(TokenType.LPAREN):
                seq = self._parse_sequence()
                return Value(span=seq.span, tag=tag, payload=seq)
            if self._check(TokenType.QUOTED, TokenType.RAW, TokenType.HEREDOC):
                scalar = self._parse_scalar()
                return Value(span=scalar.span, tag=tag, payload=scalar)
            if self._check(TokenType.AT):
                at_token = self._advance()
                return Value(span=at_token.span, tag=tag)

        return Value(span=Span(start, tag_token.span.end), tag=tag)

    def _parse_value(self) -> Value:
        """Parse a value."""
        if self._check(TokenType.AT):
            at_token = self._advance()
            if not self.current.had_whitespace_before and not self._check(
                TokenType.EOF,
                TokenType.RBRACE,
                TokenType.RPAREN,
                TokenType.COMMA,
                TokenType.LBRACE,
                TokenType.LPAREN,
            ):
                raise ParseError("invalid tag name", self.current.span)
            return Value(span=Span(at_token.span.start, at_token.span.end))

        if self._check(TokenType.TAG):
            return self._parse_tag_value()

        if self._check(TokenType.LBRACE):
            obj = self._parse_object()
            return Value(span=obj.span, payload=obj)

        if self._check(TokenType.LPAREN):
            seq = self._parse_sequence()
            return Value(span=seq.span, payload=seq)

        if self._check(TokenType.SCALAR):
            scalar_token = self._advance()
            next_token = self.current

            if next_token.type == TokenType.GT and not next_token.had_whitespace_before:
                return self._parse_attributes_starting_with(scalar_token)

            return Value(
                span=scalar_token.span,
                payload=Scalar(
                    text=scalar_token.text,
                    kind=ScalarKind.BARE,
                    span=scalar_token.span,
                ),
            )

        scalar = self._parse_scalar()
        return Value(span=scalar.span, payload=scalar)

    def _parse_attributes_starting_with(self, first_key_token: Token) -> Value:
        """Parse attribute syntax (key>value key>value ...)."""
        attrs: list[Entry] = []
        start_span = first_key_token.span

        self._expect(TokenType.GT)
        first_key = Value(
            span=first_key_token.span,
            payload=Scalar(
                text=first_key_token.text,
                kind=ScalarKind.BARE,
                span=first_key_token.span,
            ),
        )
        first_value = self._parse_attribute_value()
        attrs.append(Entry(key=first_key, value=first_value))

        end_span = first_value.span

        while self._check(TokenType.SCALAR) and not self.current.had_newline_before:
            key_token = self.current
            next_token = self._peek()
            if next_token.type != TokenType.GT or next_token.had_whitespace_before:
                break

            self._advance()
            self._advance()

            attr_key = Value(
                span=key_token.span,
                payload=Scalar(
                    text=key_token.text,
                    kind=ScalarKind.BARE,
                    span=key_token.span,
                ),
            )

            attr_value = self._parse_attribute_value()
            attrs.append(Entry(key=attr_key, value=attr_value))
            end_span = attr_value.span

        obj = StyxObject(
            entries=attrs,
            separator=Separator.COMMA,
            span=Span(start_span.start, end_span.end),
        )

        return Value(span=obj.span, payload=obj)

    def _parse_scalar(self) -> Scalar:
        """Parse a scalar value."""
        token = self.current

        match token.type:
            case TokenType.SCALAR:
                kind = ScalarKind.BARE
            case TokenType.QUOTED:
                kind = ScalarKind.QUOTED
            case TokenType.RAW:
                kind = ScalarKind.RAW
            case TokenType.HEREDOC:
                kind = ScalarKind.HEREDOC
            case _:
                raise ParseError(f"expected scalar, got {token.type.value}", token.span)

        self._advance()
        return Scalar(text=token.text, kind=kind, span=token.span)

    def _parse_object(self) -> StyxObject:
        """Parse an object."""
        open_brace = self._expect(TokenType.LBRACE)
        start = open_brace.span.start
        entries: list[Entry] = []
        separator: Separator | None = None
        seen_keys: dict[str, Span] = {}

        if self.current.had_newline_before:
            separator = Separator.NEWLINE

        while not self._check(TokenType.RBRACE, TokenType.EOF):
            entry = self._parse_entry_with_dup_check(seen_keys)
            if entry:
                entries.append(entry)

            if self._check(TokenType.COMMA):
                if separator == Separator.NEWLINE:
                    raise ParseError(
                        "mixed separators (use either commas or newlines)",
                        self.current.span,
                    )
                separator = Separator.COMMA
                self._advance()
            elif not self._check(TokenType.RBRACE, TokenType.EOF):
                if separator == Separator.COMMA:
                    raise ParseError(
                        "mixed separators (use either commas or newlines)",
                        self.current.span,
                    )
                separator = Separator.NEWLINE

        if separator is None:
            separator = Separator.COMMA

        if self._check(TokenType.EOF):
            raise ParseError("unclosed object (missing `}`)", open_brace.span)

        end = self._expect(TokenType.RBRACE).span.end
        return StyxObject(entries=entries, separator=separator, span=Span(start, end))

    def _parse_sequence(self) -> Sequence:
        """Parse a sequence."""
        open_paren = self._expect(TokenType.LPAREN)
        start = open_paren.span.start
        items: list[Value] = []

        while not self._check(TokenType.RPAREN, TokenType.EOF):
            items.append(self._parse_value())

        if self._check(TokenType.EOF):
            raise ParseError("unclosed sequence (missing `)`)", open_paren.span)

        end = self._expect(TokenType.RPAREN).span.end
        return Sequence(items=items, span=Span(start, end))


def parse(source: str) -> Document:
    """Parse a Styx document from source string."""
    return Parser(source).parse()
