package styx

import (
	"strings"
	"unicode/utf8"
)

// TokenType represents the type of a lexer token.
type TokenType int

const (
	TokenScalar TokenType = iota
	TokenQuoted
	TokenRaw
	TokenHeredoc
	TokenLBrace
	TokenRBrace
	TokenLParen
	TokenRParen
	TokenComma
	TokenAt
	TokenTag
	TokenGT
	TokenEOF
)

func (t TokenType) String() string {
	switch t {
	case TokenScalar:
		return "scalar"
	case TokenQuoted:
		return "quoted"
	case TokenRaw:
		return "raw"
	case TokenHeredoc:
		return "heredoc"
	case TokenLBrace:
		return "lbrace"
	case TokenRBrace:
		return "rbrace"
	case TokenLParen:
		return "lparen"
	case TokenRParen:
		return "rparen"
	case TokenComma:
		return "comma"
	case TokenAt:
		return "at"
	case TokenTag:
		return "tag"
	case TokenGT:
		return "gt"
	case TokenEOF:
		return "eof"
	default:
		return "unknown"
	}
}

// Token represents a lexer token.
type Token struct {
	Type                TokenType
	Text                string
	Span                Span
	HadWhitespaceBefore bool
	HadNewlineBefore    bool
}

// Lexer tokenizes Styx source code.
type Lexer struct {
	source  string
	pos     int // character position
	bytePos int // byte position for spans
}

func newLexer(source string) *Lexer {
	return &Lexer{source: source}
}

func (l *Lexer) peek(offset int) rune {
	idx := l.pos + offset
	if idx >= len(l.source) {
		return 0
	}
	r, _ := utf8.DecodeRuneInString(l.source[idx:])
	return r
}

func (l *Lexer) advance() rune {
	if l.pos >= len(l.source) {
		return 0
	}
	r, size := utf8.DecodeRuneInString(l.source[l.pos:])
	l.pos += size
	l.bytePos += size
	return r
}

func (l *Lexer) skipWhitespaceAndComments() (hadWhitespace, hadNewline bool) {
	for l.pos < len(l.source) {
		ch := l.peek(0)
		switch ch {
		case ' ', '\t', '\r':
			hadWhitespace = true
			l.advance()
		case '\n':
			hadWhitespace = true
			hadNewline = true
			l.advance()
		case '/':
			if l.peek(1) == '/' {
				hadWhitespace = true
				for l.pos < len(l.source) && l.peek(0) != '\n' {
					l.advance()
				}
			} else {
				return
			}
		default:
			return
		}
	}
	return
}

func isTagStart(ch rune) bool {
	return (ch >= 'A' && ch <= 'Z') || (ch >= 'a' && ch <= 'z') || ch == '_'
}

func isTagChar(ch rune) bool {
	return isTagStart(ch) || (ch >= '0' && ch <= '9') || ch == '-'
}

func isSpecialChar(ch rune) bool {
	switch ch {
	case '{', '}', '(', ')', ',', '"', '>', ' ', '\t', '\n', '\r':
		return true
	}
	return false
}

func (l *Lexer) nextToken() (*Token, error) {
	hadWhitespace, hadNewline := l.skipWhitespaceAndComments()

	if l.pos >= len(l.source) {
		return &Token{
			Type:                TokenEOF,
			Text:                "",
			Span:                Span{l.bytePos, l.bytePos},
			HadWhitespaceBefore: hadWhitespace,
			HadNewlineBefore:    hadNewline,
		}, nil
	}

	start := l.bytePos
	ch := l.peek(0)

	// Single-character tokens
	switch ch {
	case '{':
		l.advance()
		return &Token{TokenLBrace, "{", Span{start, l.bytePos}, hadWhitespace, hadNewline}, nil
	case '}':
		l.advance()
		return &Token{TokenRBrace, "}", Span{start, l.bytePos}, hadWhitespace, hadNewline}, nil
	case '(':
		l.advance()
		return &Token{TokenLParen, "(", Span{start, l.bytePos}, hadWhitespace, hadNewline}, nil
	case ')':
		l.advance()
		return &Token{TokenRParen, ")", Span{start, l.bytePos}, hadWhitespace, hadNewline}, nil
	case ',':
		l.advance()
		return &Token{TokenComma, ",", Span{start, l.bytePos}, hadWhitespace, hadNewline}, nil
	case '>':
		l.advance()
		return &Token{TokenGT, ">", Span{start, l.bytePos}, hadWhitespace, hadNewline}, nil
	}

	// @ - either unit or tag
	if ch == '@' {
		l.advance()
		if isTagStart(l.peek(0)) {
			nameStart := l.pos
			for isTagChar(l.peek(0)) {
				l.advance()
			}
			name := l.source[nameStart:l.pos]
			return &Token{TokenTag, name, Span{start, l.bytePos}, hadWhitespace, hadNewline}, nil
		}
		return &Token{TokenAt, "@", Span{start, l.bytePos}, hadWhitespace, hadNewline}, nil
	}

	// Quoted string
	if ch == '"' {
		return l.readQuotedString(start, hadWhitespace, hadNewline)
	}

	// Raw string
	if ch == 'r' && (l.peek(1) == '"' || l.peek(1) == '#') {
		return l.readRawString(start, hadWhitespace, hadNewline)
	}

	// Heredoc - only if << is followed by uppercase letter
	if ch == '<' && l.peek(1) == '<' {
		afterLtLt := l.peek(2)
		if afterLtLt >= 'A' && afterLtLt <= 'Z' {
			return l.readHeredoc(start, hadWhitespace, hadNewline)
		}
		// << not followed by uppercase - return error at just <<
		l.advance() // <
		l.advance() // <
		errorEnd := l.bytePos
		// Skip rest of line for recovery
		for l.pos < len(l.source) && l.peek(0) != '\n' {
			l.advance()
		}
		return nil, &ParseError{
			Message: "unexpected token",
			Span:    Span{start, errorEnd},
		}
	}

	// Bare scalar
	return l.readBareScalar(start, hadWhitespace, hadNewline)
}

func (l *Lexer) readQuotedString(start int, hadWhitespace, hadNewline bool) (*Token, error) {
	l.advance() // opening "
	var text strings.Builder

	for l.pos < len(l.source) {
		ch := l.peek(0)
		if ch == '"' {
			l.advance()
			return &Token{TokenQuoted, text.String(), Span{start, l.bytePos}, hadWhitespace, hadNewline}, nil
		}
		if ch == '\\' {
			escapeStart := l.bytePos
			l.advance()
			escaped := l.advance()
			switch escaped {
			case 'n':
				text.WriteByte('\n')
			case 'r':
				text.WriteByte('\r')
			case 't':
				text.WriteByte('\t')
			case '\\':
				text.WriteByte('\\')
			case '"':
				text.WriteByte('"')
			case 'u':
				r, err := l.readUnicodeEscape()
				if err != nil {
					return nil, err
				}
				text.WriteRune(r)
			default:
				return nil, &ParseError{
					Message: "invalid escape sequence: \\" + string(escaped),
					Span:    Span{escapeStart, l.bytePos},
				}
			}
		} else if ch == '\n' || ch == '\r' {
			// Unterminated string - include the newline in the span
			l.advance()
			return nil, &ParseError{
				Message: "unexpected token",
				Span:    Span{start, l.bytePos},
			}
		} else {
			text.WriteRune(l.advance())
		}
	}

	// EOF without closing quote - error
	return nil, &ParseError{
		Message: "unexpected token",
		Span:    Span{start, l.bytePos},
	}
}

func (l *Lexer) readUnicodeEscape() (rune, error) {
	if l.peek(0) == '{' {
		l.advance()
		var hexStr strings.Builder
		for l.peek(0) != '}' && l.pos < len(l.source) {
			hexStr.WriteRune(l.advance())
		}
		l.advance() // }
		var r rune
		_, err := parseHex(hexStr.String(), &r)
		if err != nil {
			return 0, err
		}
		return r, nil
	}

	var hexStr strings.Builder
	for i := 0; i < 4; i++ {
		hexStr.WriteRune(l.advance())
	}
	var r rune
	_, err := parseHex(hexStr.String(), &r)
	if err != nil {
		return 0, err
	}
	return r, nil
}

func parseHex(s string, r *rune) (int, error) {
	var val rune
	for _, ch := range s {
		val *= 16
		switch {
		case ch >= '0' && ch <= '9':
			val += ch - '0'
		case ch >= 'a' && ch <= 'f':
			val += ch - 'a' + 10
		case ch >= 'A' && ch <= 'F':
			val += ch - 'A' + 10
		}
	}
	*r = val
	return len(s), nil
}

func (l *Lexer) readRawString(start int, hadWhitespace, hadNewline bool) (*Token, error) {
	l.advance() // r
	hashes := 0
	for l.peek(0) == '#' {
		l.advance()
		hashes++
	}
	l.advance() // opening "

	var text strings.Builder
	closePattern := "\"" + strings.Repeat("#", hashes)

	for l.pos < len(l.source) {
		if strings.HasPrefix(l.source[l.pos:], closePattern) {
			for i := 0; i < len(closePattern); i++ {
				l.advance()
			}
			return &Token{TokenRaw, text.String(), Span{start, l.bytePos}, hadWhitespace, hadNewline}, nil
		}
		text.WriteRune(l.advance())
	}

	return nil, &ParseError{
		Message: "unclosed raw string",
		Span:    Span{start, l.bytePos},
	}
}

func (l *Lexer) readHeredoc(start int, hadWhitespace, hadNewline bool) (*Token, error) {
	l.advance() // <
	l.advance() // <

	var delimiter strings.Builder
	for l.pos < len(l.source) && l.peek(0) != '\n' {
		delimiter.WriteRune(l.advance())
	}
	if l.pos < len(l.source) {
		l.advance() // newline
	}

	// Track content start (after the opening line)
	contentStart := l.bytePos

	var text strings.Builder
	delimStr := delimiter.String()
	bareDelimiter := strings.SplitN(delimStr, ",", 2)[0]

	for l.pos < len(l.source) {
		var line strings.Builder
		for l.pos < len(l.source) && l.peek(0) != '\n' {
			line.WriteRune(l.advance())
		}

		lineStr := line.String()

		// Check for exact match (no indentation)
		if lineStr == bareDelimiter {
			result := text.String()
			return &Token{TokenHeredoc, result, Span{start, l.bytePos}, hadWhitespace, hadNewline}, nil
		}

		// Check for indented closing delimiter
		stripped := strings.TrimLeft(lineStr, " \t")
		if stripped == bareDelimiter {
			indentLen := len(lineStr) - len(stripped)
			// Dedent the content by stripping up to indentLen from each line
			result := dedentHeredoc(text.String(), indentLen)
			return &Token{TokenHeredoc, result, Span{start, l.bytePos}, hadWhitespace, hadNewline}, nil
		}

		text.WriteString(lineStr)
		if l.pos < len(l.source) && l.peek(0) == '\n' {
			l.advance()
			text.WriteByte('\n')
		}
	}

	// EOF without closing delimiter - error points at the unmatched content
	return nil, &ParseError{
		Message: "unexpected token",
		Span:    Span{contentStart, l.bytePos},
	}
}

// dedentHeredoc strips up to indentLen whitespace characters from the start of each line.
func dedentHeredoc(content string, indentLen int) string {
	lines := strings.Split(content, "\n")
	var result []string
	for _, line := range lines {
		stripped := 0
		for _, ch := range line {
			if stripped >= indentLen {
				break
			}
			if ch == ' ' || ch == '\t' {
				stripped++
			} else {
				break
			}
		}
		result = append(result, line[stripped:])
	}
	return strings.Join(result, "\n")
}

func (l *Lexer) readBareScalar(start int, hadWhitespace, hadNewline bool) (*Token, error) {
	var text strings.Builder
	for l.pos < len(l.source) {
		ch := l.peek(0)
		if isSpecialChar(ch) {
			break
		}
		text.WriteRune(l.advance())
	}
	return &Token{TokenScalar, text.String(), Span{start, l.bytePos}, hadWhitespace, hadNewline}, nil
}
