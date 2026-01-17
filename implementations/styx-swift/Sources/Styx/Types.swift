/// A span representing a range in the source text.
public struct Span: Equatable, Sendable {
    public let start: Int
    public let end: Int

    public init(start: Int, end: Int) {
        self.start = start
        self.end = end
    }

    public static let invalid = Span(start: -1, end: -1)
}

/// A parsed Styx document.
public struct Document: Equatable, Sendable {
    public var entries: [Entry]

    public init(entries: [Entry] = []) {
        self.entries = entries
    }
}

/// A key-value entry in a document or object.
public struct Entry: Equatable, Sendable {
    public var key: Value
    public var value: Value

    public init(key: Value, value: Value) {
        self.key = key
        self.value = value
    }
}

/// The kind of payload a value can have.
public enum PayloadKind: Equatable, Sendable {
    case none
    case scalar(Scalar)
    case sequence(Sequence)
    case object(Object)
}

/// A value in Styx, which may have a tag and/or a payload.
public struct Value: Equatable, Sendable {
    public var span: Span
    public var tag: Tag?
    public var payload: PayloadKind

    public init(span: Span, tag: Tag? = nil, payload: PayloadKind = .none) {
        self.span = span
        self.tag = tag
        self.payload = payload
    }

    /// Creates a unit value (no tag, no payload).
    public static func unit(span: Span) -> Value {
        Value(span: span, tag: nil, payload: .none)
    }

    /// Creates a scalar value.
    public static func scalar(_ scalar: Scalar) -> Value {
        Value(span: scalar.span, tag: nil, payload: .scalar(scalar))
    }

    /// Creates a tagged value.
    public static func tagged(_ tag: Tag, payload: PayloadKind = .none, span: Span) -> Value {
        Value(span: span, tag: tag, payload: payload)
    }
}

/// A tag annotation like @Foo.
public struct Tag: Equatable, Sendable {
    public var name: String
    public var span: Span

    public init(name: String, span: Span) {
        self.name = name
        self.span = span
    }
}

/// The kind of scalar value.
public enum ScalarKind: String, Equatable, Sendable {
    case bare
    case quoted
    case raw
    case heredoc
}

/// A scalar value (string).
public struct Scalar: Equatable, Sendable {
    public var text: String
    public var kind: ScalarKind
    public var span: Span

    public init(text: String, kind: ScalarKind, span: Span) {
        self.text = text
        self.kind = kind
        self.span = span
    }
}

/// A sequence (array) of values.
public struct Sequence: Equatable, Sendable {
    public var items: [Value]
    public var span: Span

    public init(items: [Value] = [], span: Span) {
        self.items = items
        self.span = span
    }
}

/// The separator used in an object.
public enum ObjectSeparator: String, Equatable, Sendable {
    case comma
    case newline
}

/// An object (map) of entries.
public struct Object: Equatable, Sendable {
    public var entries: [Entry]
    public var separator: ObjectSeparator
    public var span: Span

    public init(entries: [Entry] = [], separator: ObjectSeparator, span: Span) {
        self.entries = entries
        self.separator = separator
        self.span = span
    }
}

/// A parse error.
public struct ParseError: Error, Equatable, Sendable {
    public var message: String
    public var span: Span

    public init(message: String, span: Span) {
        self.message = message
        self.span = span
    }
}
