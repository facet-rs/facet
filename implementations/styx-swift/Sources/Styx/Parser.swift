/// Tracks the value kind at a path (for validation).
private enum PathValueKind {
    /// Path leads to an object (explicit `{}` or implicit from dotted path).
    case object
    /// Path leads to a terminal value (scalar, sequence, tag, unit).
    case terminal
}

/// Error type for path validation.
private enum PathError {
    case duplicate(original: Span)
    case reopened(closedPath: [String])
    case nestIntoTerminal(terminalPath: [String])
}

/// Tracks dotted path state for validation.
private struct PathState {
    /// The current open path segments.
    var currentPath: [String] = []
    /// Paths that have been closed (sibling appeared at same level).
    var closedPaths: Set<[String]> = []
    /// Full paths that have been assigned, with their value kind and span.
    var assignedPaths: [[String]: (Span, PathValueKind)] = [:]

    /// Check a path and update state. Returns error if path is invalid.
    mutating func checkAndUpdate(path: [String], span: Span, valueKind: PathValueKind) -> PathError?
    {
        // 1. Check for duplicate (exact same path)
        if let (original, _) = assignedPaths[path] {
            return .duplicate(original: original)
        }

        // 2. Check if any proper prefix is closed or has a terminal value
        for i in 1..<path.count {
            let prefix = Array(path[..<i])
            if closedPaths.contains(prefix) {
                return .reopened(closedPath: prefix)
            }
            if let (_, kind) = assignedPaths[prefix], kind == .terminal {
                return .nestIntoTerminal(terminalPath: prefix)
            }
        }

        // 3. Find common prefix length with current path
        var commonLen = 0
        for i in 0..<min(currentPath.count, path.count) {
            if currentPath[i] == path[i] {
                commonLen += 1
            } else {
                break
            }
        }

        // 4. Close paths beyond the common prefix
        for i in commonLen..<currentPath.count {
            let closed = Array(currentPath[...i])
            closedPaths.insert(closed)
        }

        // 5. Record intermediate path segments as objects (if not already assigned)
        for i in 1..<path.count {
            let prefix = Array(path[..<i])
            if assignedPaths[prefix] == nil {
                assignedPaths[prefix] = (span, .object)
            }
        }

        // 6. Update assigned paths and current path
        assignedPaths[path] = (span, valueKind)
        currentPath = path

        return nil
    }
}

/// Parser for Styx documents.
public struct Parser {
    private var lexer: Lexer
    private var current: Token
    private var previous: Token

    public init(source: String) {
        self.lexer = Lexer(source: source)
        let first = lexer.nextToken()
        self.current = first
        self.previous = first
    }

    /// Parse the source into a Document.
    public mutating func parse() throws -> Document {
        // If document starts with {, parse as single explicit root object
        // (comments are already skipped by the lexer)
        if check(.lBrace) {
            let obj = try parseObject()
            // Return a single entry with unit key and object value
            return Document(entries: [
                Entry(key: Value.unit(span: Span.invalid), value: obj)
            ])
        }

        // Otherwise parse top-level entries (implicit object at document root)
        var entries: [Entry] = []
        var pathState = PathState()

        while !check(.eof) {
            let entry = try parseEntry(pathState: &pathState)
            entries.append(entry)
        }

        return Document(entries: entries)
    }

    // MARK: - Token helpers

    private func check(_ types: TokenType...) -> Bool {
        types.contains(current.type)
    }

    private mutating func advance() -> Token {
        previous = current
        current = lexer.nextToken()
        return previous
    }

    private mutating func expect(_ type: TokenType, message: String) throws -> Token {
        if current.type == type {
            return advance()
        }
        throw ParseError(message: message, span: current.span)
    }

    // MARK: - Parsing

    private mutating func parseEntry(pathState: inout PathState) throws -> Entry {
        let key = try parseValue()

        // If an object appears at key position without a tag, it becomes the value
        // with an implicit unit key (matches Rust behavior)
        if key.tag == nil, case .object(_) = key.payload {
            let unitKey = Value.unit(span: Span(start: -1, end: -1))
            return Entry(key: unitKey, value: key)
        }

        // Validate key - reject heredocs and sequences as keys
        try validateKey(key)

        // Check for dotted path notation (e.g., server.host localhost)
        // Only applies to plain bare scalars (no tag)
        if key.tag == nil, case .scalar(let scalar) = key.payload, scalar.kind == .bare,
            scalar.text.contains(".")
        {
            return try parseDottedPathEntry(
                pathText: scalar.text, pathSpan: key.span, pathState: &pathState)
        }

        // Get key text for path tracking
        let keyText = getKeyText(key)

        // If next token is on a new line, or at end/closing delimiter, value is implicit unit
        if current.hadNewlineBefore || check(.eof, .rBrace) {
            // Check path state
            if let pathError = pathState.checkAndUpdate(
                path: [keyText], span: key.span, valueKind: .terminal)
            {
                throw pathErrorToParseError(pathError, span: key.span)
            }
            return Entry(key: key, value: Value.unit(span: key.span))
        }

        let value = try parseValue()

        // Determine value kind for path tracking
        let valueKind: PathValueKind
        switch value.payload {
        case .object(_):
            valueKind = .object
        default:
            valueKind = .terminal
        }

        // Check path state
        if let pathError = pathState.checkAndUpdate(
            path: [keyText], span: key.span, valueKind: valueKind)
        {
            throw pathErrorToParseError(pathError, span: key.span)
        }

        return Entry(key: key, value: value)
    }

    /// Extract text from a key for path tracking.
    private func getKeyText(_ key: Value) -> String {
        if let tag = key.tag {
            if case .scalar(let scalar) = key.payload {
                return "@\(tag.name)\(scalar.text)"
            }
            return "@\(tag.name)"
        }
        switch key.payload {
        case .scalar(let scalar):
            return scalar.text
        case .none:
            return "@"
        default:
            return ""
        }
    }

    /// Convert PathError to ParseError.
    private func pathErrorToParseError(_ pathError: PathError, span: Span) -> ParseError {
        switch pathError {
        case .duplicate(_):
            return ParseError(message: "duplicate key", span: span)
        case .reopened(let closedPath):
            return ParseError(
                message:
                    "cannot reopen path `\(closedPath.joined(separator: "."))` after sibling appeared",
                span: span)
        case .nestIntoTerminal(let terminalPath):
            return ParseError(
                message:
                    "cannot nest into `\(terminalPath.joined(separator: "."))` which has a terminal value",
                span: span)
        }
    }

    private func validateKey(_ key: Value) throws {
        switch key.payload {
        case .scalar(let scalar):
            // Heredocs are not valid keys
            if scalar.kind == .heredoc {
                throw ParseError(message: "invalid key", span: key.span)
            }
        case .object(_):
            // Objects at key position are OK - they become the value of an implicit unit key
            // This is handled elsewhere
            break
        case .sequence(_):
            // Sequences are not valid keys
            throw ParseError(message: "invalid key", span: key.span)
        case .none:
            // Unit value is OK as a key (e.g., @tag or just @)
            break
        }
    }

    private mutating func parseDottedPathEntry(
        pathText: String, pathSpan: Span, pathState: inout PathState
    ) throws -> Entry {
        let segments = pathText.split(separator: ".", omittingEmptySubsequences: false).map(
            String.init)

        // Check for invalid paths (empty segments)
        for seg in segments {
            if seg.isEmpty {
                throw ParseError(message: "invalid path: empty segment", span: pathSpan)
            }
        }

        // Parse the value
        let value: Value
        if current.hadNewlineBefore || check(.eof, .rBrace) {
            value = Value.unit(span: pathSpan)
        } else {
            value = try parseValue()
        }

        // Determine value kind for path tracking
        let valueKind: PathValueKind
        switch value.payload {
        case .object(_):
            valueKind = .object
        default:
            valueKind = .terminal
        }

        // Check path state for duplicates, reopening, and nesting errors
        if let pathError = pathState.checkAndUpdate(
            path: segments, span: pathSpan, valueKind: valueKind)
        {
            throw pathErrorToParseError(pathError, span: pathSpan)
        }

        // Build nested structure from inside out
        // server.host localhost -> Entry(key: server, value: Object([Entry(key: host, value: localhost)]))
        var result = value
        var currentOffset = pathSpan.start
        var segmentSpans: [(String, Span)] = []

        for seg in segments {
            // Use UTF-8 byte count for span calculation
            let segSpan = Span(start: currentOffset, end: currentOffset + seg.utf8.count)
            segmentSpans.append((seg, segSpan))
            currentOffset = segSpan.end + 1  // +1 for the dot
        }

        // Build from innermost to outermost
        // Each nested object's span starts at the PREVIOUS segment (the one whose value this object is)
        // and ends at the LAST segment key
        let lastKeyEnd = segmentSpans.last!.1.end
        for i in stride(from: segments.count - 1, to: 0, by: -1) {
            let (seg, segSpan) = segmentSpans[i]
            let keyValue = Value.scalar(Scalar(text: seg, kind: .bare, span: segSpan))
            let entry = Entry(key: keyValue, value: result)
            // Object span: starts at the PREVIOUS segment (i-1), ends at last key in path
            // This object is the VALUE of segment[i-1]
            let objStart = segmentSpans[i - 1].1.start
            let objSpan = Span(start: objStart, end: lastKeyEnd)
            let obj = Object(entries: [entry], separator: .newline, span: objSpan)
            result = Value(span: objSpan, payload: .object(obj))
        }

        // Return the outermost entry
        let (firstSeg, firstSpan) = segmentSpans[0]
        let outerKey = Value.scalar(Scalar(text: firstSeg, kind: .bare, span: firstSpan))
        return Entry(key: outerKey, value: result)
    }

    private mutating func parseValue() throws -> Value {
        // Check for lexer errors - emit them as parse errors
        if check(.error) {
            let errorToken = advance()
            throw ParseError(message: errorToken.text, span: errorToken.span)
        }

        // Check for tag
        if check(.at) {
            return try parseTaggedValue()
        }

        // Check for containers
        if check(.lBrace) {
            return try parseObject()
        }
        if check(.lParen) {
            return try parseSequence()
        }

        // Check for scalars
        if check(.bare, .quoted, .raw, .heredoc) {
            return try parseScalarOrAttributes()
        }

        // Unit value (implicit)
        return Value.unit(span: current.span)
    }

    private mutating func parseTaggedValue() throws -> Value {
        let atToken = advance()  // consume @
        let start = atToken.span.start

        // Check if followed by a bare scalar immediately adjacent (no whitespace)
        guard check(.bare) && !current.hadWhitespaceBefore else {
            // Just @ by itself = unit value
            return Value.unit(span: atToken.span)
        }

        let nameToken = advance()
        let fullText = nameToken.text

        // Check if the bare scalar contains @ (explicit unit payload marker)
        // e.g., @ok@ -> tag "ok" with explicit unit payload
        let tagNameLen: Int
        let hasTrailingAt: Bool
        if let atIndex = fullText.firstIndex(of: "@") {
            tagNameLen = fullText.distance(from: fullText.startIndex, to: atIndex)
            hasTrailingAt = true
        } else {
            tagNameLen = fullText.count
            hasTrailingAt = false
        }

        let tagName = String(fullText.prefix(tagNameLen))
        let nameEnd = nameToken.span.start + tagNameLen

        // Validate tag name
        if tagName.isEmpty {
            throw ParseError(message: "expected tag name", span: nameToken.span)
        }
        if let firstChar = tagName.first {
            if firstChar.isNumber || firstChar == "-" {
                throw ParseError(message: "invalid tag name", span: nameToken.span)
            }
        }
        for char in tagName {
            if !(char.isLetter || char.isNumber || char == "-" || char == "_") {
                throw ParseError(message: "invalid tag name", span: nameToken.span)
            }
        }

        let tag = Tag(name: tagName, span: Span(start: start, end: nameEnd))

        // If there's a trailing @ in the token, that's the explicit unit payload
        // Value span is just the payload span (the trailing @), not the whole tag
        if hasTrailingAt {
            let atPos = nameToken.span.start + tagNameLen
            return Value(span: Span(start: atPos, end: atPos + 1), tag: tag, payload: .none)
        }

        // Check for payload (must immediately follow tag name, no whitespace)
        // Value span is the payload span, not including the tag
        if check(.lBrace) && !current.hadWhitespaceBefore {
            let obj = try parseObjectInternal()
            return Value(span: obj.span, tag: tag, payload: .object(obj))
        }
        if check(.lParen) && !current.hadWhitespaceBefore {
            let seq = try parseSequenceInternal()
            return Value(span: seq.span, tag: tag, payload: .sequence(seq))
        }
        if check(.bare, .quoted, .raw, .heredoc) && !current.hadWhitespaceBefore
            && !current.hadNewlineBefore
        {
            let scalar = try parseScalarOrAttributes()
            return Value(span: scalar.span, tag: tag, payload: scalar.payload)
        }
        // @ immediately followed by another @ is tagged unit
        if check(.at) && !current.hadWhitespaceBefore {
            let unitAt = advance()
            return Value(span: unitAt.span, tag: tag, payload: .none)
        }

        // Tag with no payload (implicit unit) - span is the tag name span
        return Value(span: Span(start: start, end: nameEnd), tag: tag, payload: .none)
    }

    private mutating func parseScalarOrAttributes() throws -> Value {
        let scalarToken = advance()
        let scalar = tokenToScalar(scalarToken)

        // Check for attributes (> without whitespace)
        if check(.gt) && !current.hadWhitespaceBefore {
            return try parseAttributesStartingWith(scalar: scalar)
        }

        return Value.scalar(scalar)
    }

    private mutating func parseAttributesStartingWith(scalar: Scalar) throws -> Value {
        var entries: [Entry] = []
        let startSpan = scalar.span

        // First entry: key is the scalar, value follows after >
        _ = advance()  // consume >

        // Trailing > without value - this is an error
        if current.hadNewlineBefore || current.hadWhitespaceBefore
            || check(.eof, .rBrace, .rParen, .comma)
        {
            throw ParseError(message: "expected a value", span: previous.span)
        }

        let firstValue = try parseAttributeValue()
        entries.append(Entry(key: Value.scalar(scalar), value: firstValue))

        // Continue parsing more key>value pairs separated by whitespace
        while current.hadWhitespaceBefore && !current.hadNewlineBefore && check(.bare) {
            // Peek ahead: is this a key>value pair?
            // We need to check if after consuming the bare scalar, there's a > immediately following
            // For now, consume the bare scalar speculatively
            let keyToken = advance()

            // Check if > immediately follows (no whitespace)
            if check(.gt) && !current.hadWhitespaceBefore {
                _ = advance()  // consume >

                // Trailing > without value - this is an error
                if current.hadNewlineBefore || current.hadWhitespaceBefore
                    || check(.eof, .rBrace, .rParen, .comma)
                {
                    throw ParseError(message: "expected a value", span: previous.span)
                }

                let keyScalar = tokenToScalar(keyToken)
                let value = try parseAttributeValue()
                entries.append(Entry(key: Value.scalar(keyScalar), value: value))
            } else {
                // Not an attribute - this bare scalar is something else (maybe next entry's key)
                // We consumed it already, which is a problem. We need lookahead.
                // For now, this is a limitation - we'll break here
                // TODO: implement proper lookahead or putback
                break
            }
        }

        // Build the object from all entries
        let endSpan = entries.last?.value.span ?? startSpan
        let obj = Object(
            entries: entries,
            separator: .comma,
            span: Span(start: startSpan.start, end: endSpan.end)
        )
        return Value(span: obj.span, payload: .object(obj))
    }

    private mutating func parseAttributeValue() throws -> Value {
        if check(.at) {
            return try parseTaggedValue()
        }
        if check(.lBrace) {
            return try parseObject()
        }
        if check(.lParen) {
            return try parseSequence()
        }
        if check(.bare, .quoted, .raw, .heredoc) {
            let token = advance()
            let scalar = tokenToScalar(token)
            return Value.scalar(scalar)
        }

        throw ParseError(message: "expected a value", span: current.span)
    }

    private mutating func parseObject() throws -> Value {
        let obj = try parseObjectInternal()
        return Value(span: obj.span, payload: .object(obj))
    }

    private mutating func parseObjectInternal() throws -> Object {
        let openToken = advance()  // consume {
        let start = openToken.span.start

        var entries: [Entry] = []
        var separator: ObjectSeparator? = nil

        // Track if first entry comes after newline (for mixed separator detection)
        let firstEntryAfterNewline = current.hadNewlineBefore && !check(.rBrace, .eof)

        while !check(.rBrace, .eof) {
            // Check for newline at start of iteration (indicating newline-separated format)
            if current.hadNewlineBefore && !entries.isEmpty {
                if separator == nil {
                    separator = .newline
                } else if separator == .comma {
                    throw ParseError(
                        message: "mixed separators (use either commas or newlines)",
                        span: current.span)
                }
            }

            let key = try parseValue()
            let value = try parseValue()
            entries.append(Entry(key: key, value: value))

            // Check for comma separator
            if check(.comma) {
                // If first entry came after newline, mixing with comma is an error
                if firstEntryAfterNewline && entries.count == 1 {
                    throw ParseError(
                        message: "mixed separators (use either commas or newlines)",
                        span: current.span)
                }
                if separator == nil {
                    separator = .comma
                } else if separator != .comma {
                    throw ParseError(
                        message: "mixed separators (use either commas or newlines)",
                        span: current.span)
                }
                _ = advance()  // consume comma
            }
        }

        // Check for newline before closing brace (indicates newline format)
        if current.hadNewlineBefore && !entries.isEmpty {
            if separator == nil {
                separator = .newline
            }
        }

        // Check for unclosed object - report at opening brace position (matches Rust)
        guard check(.rBrace) else {
            throw ParseError(message: "unclosed object (missing `}`)", span: openToken.span)
        }
        let closeToken = advance()  // consume }
        return Object(
            entries: entries,
            separator: separator ?? .comma,
            span: Span(start: start, end: closeToken.span.end)
        )
    }

    private mutating func parseSequence() throws -> Value {
        let seq = try parseSequenceInternal()
        return Value(span: seq.span, payload: .sequence(seq))
    }

    private mutating func parseSequenceInternal() throws -> Sequence {
        let openToken = advance()  // consume (
        let start = openToken.span.start

        var items: [Value] = []

        while !check(.rParen, .eof) {
            // Check for comma - not allowed in sequences
            if check(.comma) {
                throw ParseError(
                    message:
                        "unexpected `,` in sequence (sequences are whitespace-separated, not comma-separated)",
                    span: current.span
                )
            }
            let item = try parseValue()
            items.append(item)
        }

        // Check for unclosed sequence - report at opening paren position (matches Rust)
        guard check(.rParen) else {
            throw ParseError(message: "unclosed sequence (missing `)`)", span: openToken.span)
        }
        let closeToken = advance()  // consume )
        return Sequence(
            items: items,
            span: Span(start: start, end: closeToken.span.end)
        )
    }

    private func tokenToScalar(_ token: Token) -> Scalar {
        let kind: ScalarKind
        switch token.type {
        case .bare: kind = .bare
        case .quoted: kind = .quoted
        case .raw: kind = .raw
        case .heredoc: kind = .heredoc
        default: kind = .bare
        }
        return Scalar(text: token.text, kind: kind, span: token.span)
    }
}
