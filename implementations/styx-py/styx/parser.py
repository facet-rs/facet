"""Parser for Styx configuration language."""

from __future__ import annotations

from .lexer import Lexer, Token, TokenType
from .types import (
    Document,
    Entry,
    ParseError,
    PathValueKind,
    Scalar,
    ScalarKind,
    Sequence,
    Span,
    StyxObject,
    Tag,
    Value,
)


class PathState:
    """Track path state for detecting reopen-path and nest-into-terminal errors."""

    __slots__ = ("assigned_paths", "closed_paths", "current_path")

    def __init__(self) -> None:
        self.current_path: list[str] = []
        self.closed_paths: set[str] = set()  # Paths that have had siblings
        self.assigned_paths: dict[str, tuple[PathValueKind, Span]] = {}

    def check_and_update(self, path: list[str], span: Span, kind: PathValueKind) -> None:
        """Check path validity and update state. Raises ParseError if invalid."""
        full_path = ".".join(path)

        # 1. Check for duplicate
        if full_path in self.assigned_paths:
            existing_kind, _existing_span = self.assigned_paths[full_path]
            if existing_kind == PathValueKind.TERMINAL:
                raise ParseError("duplicate key", span)
            # Both are objects - it's a reopen attempt
            raise ParseError(f"cannot reopen path `{full_path}` after sibling appeared", span)

        # 2. Check if any prefix is closed (has had siblings) or is terminal
        for i in range(1, len(path)):
            prefix = ".".join(path[:i])
            if prefix in self.closed_paths:
                raise ParseError(f"cannot reopen path `{prefix}` after sibling appeared", span)
            if prefix in self.assigned_paths:
                prefix_kind, _prefix_span = self.assigned_paths[prefix]
                if prefix_kind == PathValueKind.TERMINAL:
                    raise ParseError(
                        f"cannot nest into `{prefix}` which has a terminal value", span
                    )

        # 3. Find common prefix length with current path
        common_len = 0
        for i in range(min(len(path), len(self.current_path))):
            if path[i] == self.current_path[i]:
                common_len += 1
            else:
                break

        # 4. Close all divergent paths from current path
        for i in range(common_len, len(self.current_path)):
            divergent = ".".join(self.current_path[: i + 1])
            self.closed_paths.add(divergent)

        # 5. Record intermediate segments as objects
        for i in range(len(path) - 1):
            prefix = ".".join(path[: i + 1])
            if prefix not in self.assigned_paths:
                self.assigned_paths[prefix] = (PathValueKind.OBJECT, span)

        # 6. Record the final path
        self.assigned_paths[full_path] = (kind, span)
        self.current_path = path.copy()


class Parser:
    """Parser for Styx documents."""

    __slots__ = ("current", "lexer", "peeked", "source")

    def __init__(self, source: str) -> None:
        self.source = source
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
        path_state = PathState()

        # Skip any leading commas
        while self._check(TokenType.COMMA):
            self._advance()

        # Check for explicit root object: { ... } at document start
        if self._check(TokenType.LBRACE):
            # Explicit root object - parse it and check for trailing content
            obj = self._parse_object()
            obj_value = Value(span=obj.span, payload=obj)
            unit_key = Value(span=Span(-1, -1))
            entries.append(Entry(key=unit_key, value=obj_value))

            # After explicit root object, only whitespace/comments/EOF are allowed
            # Skip commas (they don't count as "content")
            while self._check(TokenType.COMMA):
                self._advance()

            if not self._check(TokenType.EOF):
                # Find the span of trailing content
                trailing_start = self.current.span.start
                # Consume tokens to find the end of all trailing content
                # Use EOF token's start as end (which includes trailing whitespace/newlines)
                while not self._check(TokenType.EOF):
                    self._advance()
                trailing_end = self.current.span.start
                raise ParseError(
                    "trailing content after explicit root object",
                    Span(trailing_start, trailing_end),
                )

            return Document(
                entries=entries,
                span=Span(start, self.current.span.end),
            )

        while not self._check(TokenType.EOF):
            entry = self._parse_entry_with_path_check(path_state)
            if entry:
                entries.append(entry)

        return Document(
            entries=entries,
            span=Span(start, self.current.span.end),
        )

    def _parse_entry_with_path_check(self, path_state: PathState) -> Entry | None:
        """Parse an entry at document level with path state checking."""
        while self._check(TokenType.COMMA):
            self._advance()

        # Stray > tokens without a value are an error
        if self._check(TokenType.GT):
            raise ParseError("expected a value", self.current.span)

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
                return self._expand_dotted_path_with_state(text, key.span, path_state)

        # Check key validity with path state
        key_text = self._get_key_text(key)

        self._validate_key(key)

        # Check for implicit unit
        if self.current.had_newline_before or self._check(TokenType.EOF, TokenType.RBRACE):
            if key_text is not None:
                path_state.check_and_update([key_text], key.span, PathValueKind.TERMINAL)
            return Entry(key=key, value=Value(span=key.span))

        value = self._parse_value()

        # Determine kind from actual value
        if key_text is not None:
            if value.payload is not None and isinstance(value.payload, StyxObject):
                kind = PathValueKind.OBJECT
            else:
                kind = PathValueKind.TERMINAL
            path_state.check_and_update([key_text], key.span, kind)

        return Entry(key=key, value=value)

    def _parse_entry_with_dup_check(self, seen_keys: dict[str, Span]) -> Entry | None:
        """Parse an entry with duplicate key checking."""
        while self._check(TokenType.COMMA):
            self._advance()

        # Stray > tokens without a value are an error
        if self._check(TokenType.GT):
            raise ParseError("expected a value", self.current.span)

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
                # Point at just the opening marker (<<TAG), not the whole content
                error_span = self._heredoc_start_span(key.payload.span)
                raise ParseError("invalid key", error_span)

    def _heredoc_start_span(self, heredoc_span: Span) -> Span:
        """Get the span of just the heredoc opening marker (<<TAG\\n)."""
        text = self.source[heredoc_span.start : heredoc_span.end]
        newline_idx = text.find("\n")
        end_offset = newline_idx + 1 if newline_idx >= 0 else len(text)
        return Span(heredoc_span.start, heredoc_span.start + end_offset)

    def _expand_dotted_path_with_state(
        self, path_text: str, span: Span, path_state: PathState
    ) -> Entry:
        """Expand a dotted path into nested objects with path state validation."""
        segments = path_text.split(".")

        if any(s == "" for s in segments):
            raise ParseError("invalid key", span)

        # Calculate spans for each segment
        segment_spans: list[Span] = []
        offset = span.start
        for segment in segments:
            segment_bytes = len(segment.encode("utf-8"))
            segment_spans.append(Span(offset, offset + segment_bytes))
            offset += segment_bytes + 1  # +1 for the dot

        # Parse the value
        value = self._parse_value()

        # Determine value kind
        if value.payload is not None and isinstance(value.payload, StyxObject):
            kind = PathValueKind.OBJECT
        else:
            kind = PathValueKind.TERMINAL

        # Check and update path state - use full path span for error messages
        path_state.check_and_update(segments, span, kind)

        # Build nested objects from inside out
        # Object spans start at the PREVIOUS segment's position (i-1)
        last_key_end = segment_spans[-1].end
        result = value
        for i in range(len(segments) - 1, 0, -1):
            seg_span = segment_spans[i]
            segment_key = Value(
                span=seg_span,
                payload=Scalar(text=segments[i], kind=ScalarKind.BARE, span=seg_span),
            )
            # Object span starts at the previous segment's position
            obj_start = segment_spans[i - 1].start
            obj_span = Span(obj_start, last_key_end)
            result = Value(
                span=obj_span,
                payload=StyxObject(
                    entries=[Entry(key=segment_key, value=result)],
                    span=obj_span,
                ),
            )

        first_span = segment_spans[0]
        outer_key = Value(
            span=first_span,
            payload=Scalar(text=segments[0], kind=ScalarKind.BARE, span=first_span),
        )

        return Entry(key=outer_key, value=result)

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
            # If there's something else immediately after the tag (like /package),
            # it's an invalid tag name. Span starts at the @.
            if not self._check(
                TokenType.EOF,
                TokenType.RBRACE,
                TokenType.RPAREN,
                TokenType.COMMA,
            ):
                raise ParseError("invalid tag name", Span(start, self.current.span.end))

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
                # Error span includes the @ (it's part of the tag)
                raise ParseError(
                    "invalid tag name", Span(at_token.span.start, self.current.span.end)
                )
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

            # Attribute syntax: scalar>value - but only if there's actually a value after >
            # If > is at EOF or followed by newline, just treat scalar as the value
            if (
                next_token.type == TokenType.GT
                and not next_token.had_whitespace_before
                and not self._peek().had_newline_before
                and self._peek().type != TokenType.EOF
            ):
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
        seen_keys: dict[str, Span] = {}

        while not self._check(TokenType.RBRACE, TokenType.EOF):
            entry = self._parse_entry_with_dup_check(seen_keys)
            if entry:
                entries.append(entry)

            # Skip commas (mixed separators now allowed)
            if self._check(TokenType.COMMA):
                self._advance()

        if self._check(TokenType.EOF):
            raise ParseError("unclosed object (missing `}`)", open_brace.span)

        end = self._expect(TokenType.RBRACE).span.end
        return StyxObject(entries=entries, span=Span(start, end))

    def _parse_sequence(self) -> Sequence:
        """Parse a sequence."""
        open_paren = self._expect(TokenType.LPAREN)
        start = open_paren.span.start
        items: list[Value] = []

        while not self._check(TokenType.RPAREN, TokenType.EOF):
            # Check for comma - not allowed in sequences
            if self._check(TokenType.COMMA):
                raise ParseError(
                    "unexpected `,` in sequence (sequences are whitespace-separated, not comma-separated)",
                    self.current.span,
                )
            items.append(self._parse_value())

        if self._check(TokenType.EOF):
            raise ParseError("unclosed sequence (missing `)`)", open_paren.span)

        end = self._expect(TokenType.RPAREN).span.end
        return Sequence(items=items, span=Span(start, end))


def parse(source: str) -> Document:
    """Parse a Styx document from source string."""
    return Parser(source).parse()
