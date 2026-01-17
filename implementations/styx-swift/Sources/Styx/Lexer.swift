/// Token types in Styx.
public enum TokenType: Equatable, Sendable {
    case eof
    case newline
    case lBrace      // {
    case rBrace      // }
    case lParen      // (
    case rParen      // )
    case comma       // ,
    case gt          // >
    case at          // @
    case bare        // bare identifier/scalar
    case quoted      // "string"
    case raw         // r#"string"#
    case heredoc     // <<TAG...TAG
    case error
}

/// A token from the lexer.
public struct Token: Equatable, Sendable {
    public var type: TokenType
    public var span: Span
    public var text: String
    public var hadWhitespaceBefore: Bool
    public var hadNewlineBefore: Bool

    public init(type: TokenType, span: Span, text: String = "", hadWhitespaceBefore: Bool = false, hadNewlineBefore: Bool = false) {
        self.type = type
        self.span = span
        self.text = text
        self.hadWhitespaceBefore = hadWhitespaceBefore
        self.hadNewlineBefore = hadNewlineBefore
    }
}

/// Lexer for Styx source text.
public struct Lexer {
    private let source: String
    private var index: String.Index
    private var position: Int
    private var hadWhitespace: Bool = false
    private var hadNewline: Bool = false
    private var pendingNewline: Bool = false  // For heredocs that end with newline

    public init(source: String) {
        self.source = source
        self.index = source.startIndex
        self.position = 0
    }

    private var isAtEnd: Bool {
        index >= source.endIndex
    }

    private func peek() -> Character? {
        guard !isAtEnd else { return nil }
        return source[index]
    }

    private func peekNext() -> Character? {
        guard !isAtEnd else { return nil }
        let next = source.index(after: index)
        guard next < source.endIndex else { return nil }
        return source[next]
    }

    /// Returns the character two positions ahead (after peekNext).
    private func peekAfterNext() -> Character? {
        guard !isAtEnd else { return nil }
        let next = source.index(after: index)
        guard next < source.endIndex else { return nil }
        let afterNext = source.index(after: next)
        guard afterNext < source.endIndex else { return nil }
        return source[afterNext]
    }

    private mutating func advance() -> Character? {
        guard !isAtEnd else { return nil }
        let char = source[index]
        index = source.index(after: index)
        position += 1
        return char
    }

    private mutating func skipWhitespaceAndComments() {
        hadWhitespace = false
        hadNewline = pendingNewline
        pendingNewline = false

        while !isAtEnd {
            guard let char = peek() else { break }

            if char == " " || char == "\t" || char == "\r" {
                hadWhitespace = true
                _ = advance()
            } else if char == "\n" {
                hadWhitespace = true
                hadNewline = true
                _ = advance()
            } else if char == "/" && peekNext() == "/" {
                // Line comment
                while let c = peek(), c != "\n" {
                    _ = advance()
                }
            } else {
                break
            }
        }
    }

    public mutating func nextToken() -> Token {
        skipWhitespaceAndComments()

        let ws = hadWhitespace
        let nl = hadNewline

        guard !isAtEnd else {
            return Token(type: .eof, span: Span(start: position, end: position), hadWhitespaceBefore: ws, hadNewlineBefore: nl)
        }

        let start = position
        guard let char = advance() else {
            return Token(type: .eof, span: Span(start: start, end: start), hadWhitespaceBefore: ws, hadNewlineBefore: nl)
        }

        switch char {
        case "{":
            return Token(type: .lBrace, span: Span(start: start, end: position), hadWhitespaceBefore: ws, hadNewlineBefore: nl)
        case "}":
            return Token(type: .rBrace, span: Span(start: start, end: position), hadWhitespaceBefore: ws, hadNewlineBefore: nl)
        case "(":
            return Token(type: .lParen, span: Span(start: start, end: position), hadWhitespaceBefore: ws, hadNewlineBefore: nl)
        case ")":
            return Token(type: .rParen, span: Span(start: start, end: position), hadWhitespaceBefore: ws, hadNewlineBefore: nl)
        case ",":
            return Token(type: .comma, span: Span(start: start, end: position), hadWhitespaceBefore: ws, hadNewlineBefore: nl)
        case ">":
            return Token(type: .gt, span: Span(start: start, end: position), hadWhitespaceBefore: ws, hadNewlineBefore: nl)
        case "@":
            return Token(type: .at, span: Span(start: start, end: position), hadWhitespaceBefore: ws, hadNewlineBefore: nl)
        case "\"":
            return lexQuotedString(start: start, ws: ws, nl: nl)
        case "r" where peek() == "#" || peek() == "\"":
            return lexRawString(start: start, ws: ws, nl: nl)
        case "<" where peek() == "<" && peekAfterNext()?.isUppercase == true:
            // Only start heredoc if <<UPPERCASE follows
            return lexHeredoc(start: start, ws: ws, nl: nl)
        case "<" where peek() == "<":
            // << not followed by uppercase is an error
            _ = advance() // consume second <
            return Token(type: .error, span: Span(start: start, end: position), text: "invalid heredoc: delimiter must start with uppercase letter", hadWhitespaceBefore: ws, hadNewlineBefore: nl)
        case "/" where peek() != "/":
            // Single / starts a bare scalar (e.g., /etc/config)
            // But // is a comment (handled in skipWhitespaceAndComments)
            return lexBare(start: start, firstChar: char, ws: ws, nl: nl)
        default:
            if char.canStartBare {
                return lexBare(start: start, firstChar: char, ws: ws, nl: nl)
            } else {
                // Error: character can't start a value
                return Token(type: .error, span: Span(start: start, end: position), text: "unexpected character '\(char)'", hadWhitespaceBefore: ws, hadNewlineBefore: nl)
            }
        }
    }

    private mutating func lexQuotedString(start: Int, ws: Bool, nl: Bool) -> Token {
        var text = ""
        var closed = false

        while !isAtEnd {
            guard let char = advance() else { break }

            if char == "\"" {
                closed = true
                break
            } else if char == "\\" {
                if let escaped = advance() {
                    switch escaped {
                    case "n": text.append("\n")
                    case "r": text.append("\r")
                    case "t": text.append("\t")
                    case "\\": text.append("\\")
                    case "\"": text.append("\"")
                    case "u":
                        if let unicodeChar = parseUnicodeEscape() {
                            text.append(unicodeChar)
                        } else {
                            return Token(type: .error, span: Span(start: start, end: position), text: "invalid unicode escape", hadWhitespaceBefore: ws, hadNewlineBefore: nl)
                        }
                    default:
                        return Token(type: .error, span: Span(start: start, end: position), text: "invalid escape sequence", hadWhitespaceBefore: ws, hadNewlineBefore: nl)
                    }
                }
            } else if char == "\n" {
                return Token(type: .error, span: Span(start: start, end: position), text: "unterminated string", hadWhitespaceBefore: ws, hadNewlineBefore: nl)
            } else {
                text.append(char)
            }
        }

        if !closed {
            return Token(type: .error, span: Span(start: start, end: position), text: "unterminated string", hadWhitespaceBefore: ws, hadNewlineBefore: nl)
        }

        return Token(type: .quoted, span: Span(start: start, end: position), text: text, hadWhitespaceBefore: ws, hadNewlineBefore: nl)
    }

    private mutating func parseUnicodeEscape() -> Character? {
        guard let first = peek() else { return nil }

        if first == "{" {
            _ = advance() // consume {
            var hex = ""
            while let c = peek(), c != "}" {
                hex.append(c)
                _ = advance()
            }
            guard peek() == "}" else { return nil }
            _ = advance() // consume }

            guard hex.count >= 1 && hex.count <= 6,
                  let codepoint = UInt32(hex, radix: 16),
                  let scalar = Unicode.Scalar(codepoint) else { return nil }
            return Character(scalar)
        } else {
            // 4-digit form
            var hex = ""
            for _ in 0..<4 {
                guard let c = peek(), c.isHexDigit else { return nil }
                hex.append(c)
                _ = advance()
            }
            guard let codepoint = UInt32(hex, radix: 16),
                  let scalar = Unicode.Scalar(codepoint) else { return nil }
            return Character(scalar)
        }
    }

    private mutating func lexRawString(start: Int, ws: Bool, nl: Bool) -> Token {
        // Count opening hashes
        var hashCount = 0
        while peek() == "#" {
            _ = advance()
            hashCount += 1
        }

        guard peek() == "\"" else {
            return Token(type: .error, span: Span(start: start, end: position), text: "expected \" after r#", hadWhitespaceBefore: ws, hadNewlineBefore: nl)
        }
        _ = advance() // consume opening "

        var text = ""
        while !isAtEnd {
            guard let char = advance() else { break }

            if char == "\"" {
                // Check for closing hashes
                var closingHashes = 0
                let saveIndex = index
                let savePos = position
                while closingHashes < hashCount && peek() == "#" {
                    _ = advance()
                    closingHashes += 1
                }
                if closingHashes == hashCount {
                    return Token(type: .raw, span: Span(start: start, end: position), text: text, hadWhitespaceBefore: ws, hadNewlineBefore: nl)
                }
                // Not the end, restore and include the quote
                index = saveIndex
                position = savePos
                text.append(char)
            } else {
                text.append(char)
            }
        }

        return Token(type: .error, span: Span(start: start, end: position), text: "unterminated raw string", hadWhitespaceBefore: ws, hadNewlineBefore: nl)
    }

    private mutating func lexHeredoc(start: Int, ws: Bool, nl: Bool) -> Token {
        _ = advance() // consume second <

        // Delimiter: uppercase letters, digits, underscores
        // First char must be uppercase letter
        guard let firstChar = peek(), firstChar.isUppercase else {
            // Error recovery: consume any delimiter-like chars
            while let c = peek(), c.isUppercase || c.isNumber || c == "_" {
                _ = advance()
            }
            return Token(type: .error, span: Span(start: start, end: position), text: "heredoc delimiter must start with uppercase letter", hadWhitespaceBefore: ws, hadNewlineBefore: nl)
        }

        var delimiter = ""
        while let c = peek(), c.isUppercase || c.isNumber || c == "_" {
            delimiter.append(c)
            _ = advance()
        }

        if delimiter.count > 16 {
            return Token(type: .error, span: Span(start: start, end: position), text: "heredoc delimiter too long", hadWhitespaceBefore: ws, hadNewlineBefore: nl)
        }

        // Consume optional \r and \n after delimiter
        if peek() == "\r" {
            _ = advance()
        }
        if peek() == "\n" {
            _ = advance()
        }

        // Read content until we find the delimiter on its own line
        var text = ""
        var currentLine = ""

        while !isAtEnd {
            guard let char = advance() else { break }

            if char == "\n" {
                // Check if currentLine matches the delimiter (with optional leading whitespace)
                let trimmed = currentLine.trimmingLeadingWhitespace()
                if trimmed == delimiter {
                    // Found closing delimiter - span ends before the final newline
                    // Mark that next token should have hadNewlineBefore = true
                    pendingNewline = true
                    return Token(type: .heredoc, span: Span(start: start, end: position - 1), text: text, hadWhitespaceBefore: ws, hadNewlineBefore: nl)
                }
                text.append(contentsOf: currentLine)
                text.append("\n")
                currentLine = ""
            } else {
                currentLine.append(char)
            }
        }

        // Check if final line (without trailing newline) is the delimiter
        let trimmed = currentLine.trimmingLeadingWhitespace()
        if trimmed == delimiter {
            // No trailing newline, span ends at current position
            return Token(type: .heredoc, span: Span(start: start, end: position), text: text, hadWhitespaceBefore: ws, hadNewlineBefore: nl)
        }

        return Token(type: .error, span: Span(start: start, end: position), text: "unterminated heredoc", hadWhitespaceBefore: ws, hadNewlineBefore: nl)
    }

    private mutating func lexBare(start: Int, firstChar: Character, ws: Bool, nl: Bool) -> Token {
        var text = String(firstChar)

        while let c = peek() {
            if c.isBareChar {
                text.append(c)
                _ = advance()
            } else {
                break
            }
        }

        return Token(type: .bare, span: Span(start: start, end: position), text: text, hadWhitespaceBefore: ws, hadNewlineBefore: nl)
    }
}

extension Character {
    /// Can this character START a bare scalar?
    /// `@`, `=`, and `/` are NOT allowed at the start
    var canStartBare: Bool {
        if self.isWhitespace { return false }
        switch self {
        case "{", "}", "(", ")", ",", ">", "@", "=", "/", "\"", "\n", "\r":
            return false
        default:
            return true
        }
    }

    /// Can this character CONTINUE a bare scalar (after the first char)?
    /// `@`, `=`, and `/` ARE allowed after the first char
    /// `>` is NEVER allowed (attribute separator)
    var isBareChar: Bool {
        if self.isWhitespace { return false }
        switch self {
        case "{", "}", "(", ")", ",", ">", "\"", "\n", "\r":
            return false
        default:
            return true
        }
    }

    var isHexDigit: Bool {
        switch self {
        case "0"..."9", "a"..."f", "A"..."F":
            return true
        default:
            return false
        }
    }
}

extension String {
    func trimmingLeadingWhitespace() -> String {
        guard let firstNonWhitespace = self.firstIndex(where: { !$0.isWhitespace }) else {
            return ""
        }
        return String(self[firstNonWhitespace...])
    }
}
