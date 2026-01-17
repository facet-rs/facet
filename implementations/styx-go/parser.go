package styx

import (
	"strings"
)

type parser struct {
	lexer   *Lexer
	current *Token
	peeked  *Token
	err     error
}

func newParser(source string) *parser {
	p := &parser{lexer: newLexer(source)}
	tok, err := p.lexer.nextToken()
	if err != nil {
		p.err = err
		p.current = &Token{Type: TokenEOF, Span: Span{0, 0}}
	} else {
		p.current = tok
	}
	return p
}

func (p *parser) advance() *Token {
	prev := p.current
	if p.peeked != nil {
		p.current = p.peeked
		p.peeked = nil
	} else {
		tok, err := p.lexer.nextToken()
		if err != nil {
			p.err = err
			p.current = &Token{Type: TokenEOF, Span: Span{p.lexer.bytePos, p.lexer.bytePos}}
		} else {
			p.current = tok
		}
	}
	return prev
}

func (p *parser) peek() *Token {
	if p.peeked == nil {
		tok, err := p.lexer.nextToken()
		if err != nil {
			p.err = err
			p.peeked = &Token{Type: TokenEOF, Span: Span{p.lexer.bytePos, p.lexer.bytePos}}
		} else {
			p.peeked = tok
		}
	}
	return p.peeked
}

func (p *parser) check(types ...TokenType) bool {
	for _, t := range types {
		if p.current.Type == t {
			return true
		}
	}
	return false
}

func (p *parser) expect(tokenType TokenType) (*Token, error) {
	if p.current.Type != tokenType {
		return nil, &ParseError{
			Message: "expected " + tokenType.String() + ", got " + p.current.Type.String(),
			Span:    p.current.Span,
		}
	}
	return p.advance(), nil
}

func (p *parser) parse() (*Document, error) {
	if p.err != nil {
		return nil, p.err
	}

	entries := []*Entry{}
	start := p.current.Span.Start
	seenKeys := make(map[string]Span)

	for !p.check(TokenEOF) {
		if p.err != nil {
			return nil, p.err
		}
		entry, err := p.parseEntryWithDupCheck(seenKeys)
		if err != nil {
			return nil, err
		}
		if entry != nil {
			entries = append(entries, entry)
		}
	}

	return &Document{
		Entries: entries,
		Span:    Span{start, p.current.Span.End},
	}, nil
}

func (p *parser) parseEntryWithDupCheck(seenKeys map[string]Span) (*Entry, error) {
	for p.check(TokenComma) {
		p.advance()
	}

	if p.err != nil {
		return nil, p.err
	}

	if p.check(TokenEOF, TokenRBrace) {
		return nil, nil
	}

	key, err := p.parseValue()
	if err != nil {
		return nil, err
	}
	if p.err != nil {
		return nil, p.err
	}

	// Special case: object in key position gets implicit unit key
	if key.PayloadKind == PayloadObject {
		if !p.current.HadNewlineBefore && !p.check(TokenEOF, TokenRBrace, TokenComma) {
			p.parseValue() // Drop trailing value
		}
		unitKey := &Value{Span: Span{-1, -1}}
		return &Entry{Key: unitKey, Value: key}, nil
	}

	// Check for dotted path in bare scalar key
	if key.PayloadKind == PayloadScalar && key.Scalar.Kind == ScalarBare {
		text := key.Scalar.Text
		if strings.Contains(text, ".") {
			return p.expandDottedPath(text, key.Span, seenKeys)
		}
	}

	// Check for duplicate key
	keyText := p.getKeyText(key)
	if keyText != "" {
		if _, exists := seenKeys[keyText]; exists {
			return nil, &ParseError{Message: "duplicate key", Span: key.Span}
		}
		seenKeys[keyText] = key.Span
	}

	if err := p.validateKey(key); err != nil {
		return nil, err
	}

	// Check for implicit unit
	if p.current.HadNewlineBefore || p.check(TokenEOF, TokenRBrace) {
		return &Entry{Key: key, Value: &Value{Span: key.Span}}, nil
	}

	value, err := p.parseValue()
	if err != nil {
		return nil, err
	}
	return &Entry{Key: key, Value: value}, nil
}

func (p *parser) getKeyText(key *Value) string {
	if key.PayloadKind == PayloadScalar {
		return key.Scalar.Text
	}
	if key.Tag != nil && key.PayloadKind == PayloadNone {
		return "@" + key.Tag.Name
	}
	return ""
}

func (p *parser) validateKey(key *Value) error {
	if key.PayloadKind == PayloadSequence {
		return &ParseError{Message: "invalid key", Span: key.Span}
	}
	if key.PayloadKind == PayloadScalar && key.Scalar.Kind == ScalarHeredoc {
		return &ParseError{Message: "invalid key", Span: key.Span}
	}
	return nil
}

func (p *parser) expandDottedPath(pathText string, span Span, seenKeys map[string]Span) (*Entry, error) {
	segments := strings.Split(pathText, ".")

	for _, s := range segments {
		if s == "" {
			return nil, &ParseError{Message: "invalid key", Span: span}
		}
	}

	firstSegment := segments[0]
	if _, exists := seenKeys[firstSegment]; exists {
		return nil, &ParseError{Message: "duplicate key", Span: span}
	}
	seenKeys[firstSegment] = span

	// Calculate spans for each segment
	segmentSpans := make([]Span, len(segments))
	offset := span.Start
	for i, segment := range segments {
		segmentBytes := len(segment)
		segmentSpans[i] = Span{offset, offset + segmentBytes}
		offset += segmentBytes + 1 // +1 for the dot
	}

	value, err := p.parseValue()
	if err != nil {
		return nil, err
	}

	// Build nested structure from inside out
	result := value
	for i := len(segments) - 1; i > 0; i-- {
		segSpan := segmentSpans[i]
		segmentKey := &Value{
			Span:        segSpan,
			PayloadKind: PayloadScalar,
			Scalar:      &Scalar{Text: segments[i], Kind: ScalarBare, Span: segSpan},
		}
		result = &Value{
			Span:        span,
			PayloadKind: PayloadObject,
			Object: &Object{
				Entries:   []*Entry{{Key: segmentKey, Value: result}},
				Separator: SeparatorNewline,
				Span:      span,
			},
		}
	}

	firstSpan := segmentSpans[0]
	outerKey := &Value{
		Span:        firstSpan,
		PayloadKind: PayloadScalar,
		Scalar:      &Scalar{Text: firstSegment, Kind: ScalarBare, Span: firstSpan},
	}

	return &Entry{Key: outerKey, Value: result}, nil
}

func (p *parser) parseAttributeValue() (*Value, error) {
	if p.check(TokenLBrace) {
		obj, err := p.parseObject()
		if err != nil {
			return nil, err
		}
		return &Value{Span: obj.Span, PayloadKind: PayloadObject, Object: obj}, nil
	}
	if p.check(TokenLParen) {
		seq, err := p.parseSequence()
		if err != nil {
			return nil, err
		}
		return &Value{Span: seq.Span, PayloadKind: PayloadSequence, Sequence: seq}, nil
	}
	if p.check(TokenTag) {
		return p.parseTagValue()
	}
	if p.check(TokenAt) {
		atToken := p.advance()
		return &Value{Span: atToken.Span}, nil
	}
	scalar, err := p.parseScalar()
	if err != nil {
		return nil, err
	}
	return &Value{Span: scalar.Span, PayloadKind: PayloadScalar, Scalar: scalar}, nil
}

func (p *parser) parseTagValue() (*Value, error) {
	start := p.current.Span.Start
	tagToken := p.advance()
	tag := &Tag{Name: tagToken.Text, Span: tagToken.Span}

	if !p.current.HadWhitespaceBefore {
		// Check for invalid tag continuation (e.g., @org/package where / is not a valid tag char)
		if p.check(TokenScalar) {
			// There's a scalar immediately after the tag without whitespace
			// This means there was a character that broke the tag name (like /)
			// Error span starts after the @ (at the tag name) and ends at the invalid scalar
			return nil, &ParseError{
				Message: "invalid tag name",
				Span:    Span{start + 1, p.current.Span.End},
			}
		}
		if p.check(TokenLBrace) {
			obj, err := p.parseObject()
			if err != nil {
				return nil, err
			}
			return &Value{Span: obj.Span, Tag: tag, PayloadKind: PayloadObject, Object: obj}, nil
		}
		if p.check(TokenLParen) {
			seq, err := p.parseSequence()
			if err != nil {
				return nil, err
			}
			return &Value{Span: seq.Span, Tag: tag, PayloadKind: PayloadSequence, Sequence: seq}, nil
		}
		if p.check(TokenQuoted, TokenRaw, TokenHeredoc) {
			scalar, err := p.parseScalar()
			if err != nil {
				return nil, err
			}
			return &Value{Span: scalar.Span, Tag: tag, PayloadKind: PayloadScalar, Scalar: scalar}, nil
		}
		if p.check(TokenAt) {
			atToken := p.advance()
			return &Value{Span: atToken.Span, Tag: tag}, nil
		}
	}

	return &Value{Span: Span{start, tagToken.Span.End}, Tag: tag}, nil
}

func (p *parser) parseValue() (*Value, error) {
	if p.err != nil {
		return nil, p.err
	}

	if p.check(TokenAt) {
		atToken := p.advance()
		if !p.current.HadWhitespaceBefore && !p.check(TokenEOF, TokenRBrace, TokenRParen, TokenComma, TokenLBrace, TokenLParen) {
			return nil, &ParseError{Message: "invalid tag name", Span: p.current.Span}
		}
		return &Value{Span: Span{atToken.Span.Start, atToken.Span.End}}, nil
	}

	if p.check(TokenTag) {
		return p.parseTagValue()
	}

	if p.check(TokenLBrace) {
		obj, err := p.parseObject()
		if err != nil {
			return nil, err
		}
		return &Value{Span: obj.Span, PayloadKind: PayloadObject, Object: obj}, nil
	}

	if p.check(TokenLParen) {
		seq, err := p.parseSequence()
		if err != nil {
			return nil, err
		}
		return &Value{Span: seq.Span, PayloadKind: PayloadSequence, Sequence: seq}, nil
	}

	if p.check(TokenScalar) {
		scalarToken := p.advance()
		nextToken := p.current

		if nextToken.Type == TokenGT && !nextToken.HadWhitespaceBefore {
			// Peek ahead: if > is followed by newline/EOF, just consume the > and return scalar
			// Otherwise, parse as attributes
			p.advance() // consume >
			afterGT := p.current
			if afterGT.HadNewlineBefore || p.check(TokenEOF, TokenRBrace, TokenRParen) {
				// > at end of line - return just the scalar
				return &Value{
					Span:        scalarToken.Span,
					PayloadKind: PayloadScalar,
					Scalar: &Scalar{
						Text: scalarToken.Text,
						Kind: ScalarBare,
						Span: scalarToken.Span,
					},
				}, nil
			}
			// Not end of line - parse as attributes (we already consumed >)
			return p.parseAttributesAfterGT(scalarToken)
		}

		return &Value{
			Span:        scalarToken.Span,
			PayloadKind: PayloadScalar,
			Scalar: &Scalar{
				Text: scalarToken.Text,
				Kind: ScalarBare,
				Span: scalarToken.Span,
			},
		}, nil
	}

	scalar, err := p.parseScalar()
	if err != nil {
		return nil, err
	}
	return &Value{Span: scalar.Span, PayloadKind: PayloadScalar, Scalar: scalar}, nil
}

func (p *parser) parseAttributesStartingWith(firstKeyToken *Token) (*Value, error) {
	attrs := []*Entry{}
	startSpan := firstKeyToken.Span

	p.expect(TokenGT)
	firstKey := &Value{
		Span:        firstKeyToken.Span,
		PayloadKind: PayloadScalar,
		Scalar: &Scalar{
			Text: firstKeyToken.Text,
			Kind: ScalarBare,
			Span: firstKeyToken.Span,
		},
	}
	firstValue, err := p.parseAttributeValue()
	if err != nil {
		return nil, err
	}
	attrs = append(attrs, &Entry{Key: firstKey, Value: firstValue})

	endSpan := firstValue.Span

	for p.check(TokenScalar) && !p.current.HadNewlineBefore {
		keyToken := p.current
		nextToken := p.peek()
		if nextToken.Type != TokenGT || nextToken.HadWhitespaceBefore {
			break
		}

		p.advance()
		p.advance()

		attrKey := &Value{
			Span:        keyToken.Span,
			PayloadKind: PayloadScalar,
			Scalar: &Scalar{
				Text: keyToken.Text,
				Kind: ScalarBare,
				Span: keyToken.Span,
			},
		}

		attrValue, err := p.parseAttributeValue()
		if err != nil {
			return nil, err
		}
		attrs = append(attrs, &Entry{Key: attrKey, Value: attrValue})
		endSpan = attrValue.Span
	}

	obj := &Object{
		Entries:   attrs,
		Separator: SeparatorComma,
		Span:      Span{startSpan.Start, endSpan.End},
	}

	return &Value{Span: obj.Span, PayloadKind: PayloadObject, Object: obj}, nil
}

func (p *parser) parseAttributesAfterGT(firstKeyToken *Token) (*Value, error) {
	// Same as parseAttributesStartingWith but > was already consumed
	attrs := []*Entry{}
	startSpan := firstKeyToken.Span

	firstKey := &Value{
		Span:        firstKeyToken.Span,
		PayloadKind: PayloadScalar,
		Scalar: &Scalar{
			Text: firstKeyToken.Text,
			Kind: ScalarBare,
			Span: firstKeyToken.Span,
		},
	}
	firstValue, err := p.parseAttributeValue()
	if err != nil {
		return nil, err
	}
	attrs = append(attrs, &Entry{Key: firstKey, Value: firstValue})

	endSpan := firstValue.Span

	for p.check(TokenScalar) && !p.current.HadNewlineBefore {
		keyToken := p.current
		nextToken := p.peek()
		if nextToken.Type != TokenGT || nextToken.HadWhitespaceBefore {
			break
		}

		p.advance()
		p.advance()

		attrKey := &Value{
			Span:        keyToken.Span,
			PayloadKind: PayloadScalar,
			Scalar: &Scalar{
				Text: keyToken.Text,
				Kind: ScalarBare,
				Span: keyToken.Span,
			},
		}

		attrValue, err := p.parseAttributeValue()
		if err != nil {
			return nil, err
		}
		attrs = append(attrs, &Entry{Key: attrKey, Value: attrValue})
		endSpan = attrValue.Span
	}

	obj := &Object{
		Entries:   attrs,
		Separator: SeparatorComma,
		Span:      Span{startSpan.Start, endSpan.End},
	}

	return &Value{Span: obj.Span, PayloadKind: PayloadObject, Object: obj}, nil
}

func (p *parser) parseScalar() (*Scalar, error) {
	token := p.current

	var kind ScalarKind
	switch token.Type {
	case TokenScalar:
		kind = ScalarBare
	case TokenQuoted:
		kind = ScalarQuoted
	case TokenRaw:
		kind = ScalarRaw
	case TokenHeredoc:
		kind = ScalarHeredoc
	default:
		return nil, &ParseError{
			Message: "expected scalar, got " + token.Type.String(),
			Span:    token.Span,
		}
	}

	p.advance()
	return &Scalar{Text: token.Text, Kind: kind, Span: token.Span}, nil
}

func (p *parser) parseObject() (*Object, error) {
	openBrace, err := p.expect(TokenLBrace)
	if err != nil {
		return nil, err
	}
	start := openBrace.Span.Start
	entries := []*Entry{}
	var separator Separator
	hasSeparator := false
	seenKeys := make(map[string]Span)

	if p.current.HadNewlineBefore {
		separator = SeparatorNewline
		hasSeparator = true
	}

	for !p.check(TokenRBrace, TokenEOF) {
		entry, err := p.parseEntryWithDupCheck(seenKeys)
		if err != nil {
			return nil, err
		}
		if entry != nil {
			entries = append(entries, entry)
		}

		if p.check(TokenComma) {
			if hasSeparator && separator == SeparatorNewline {
				return nil, &ParseError{
					Message: "mixed separators (use either commas or newlines)",
					Span:    p.current.Span,
				}
			}
			separator = SeparatorComma
			hasSeparator = true
			p.advance()
		} else if !p.check(TokenRBrace, TokenEOF) {
			if hasSeparator && separator == SeparatorComma {
				return nil, &ParseError{
					Message: "mixed separators (use either commas or newlines)",
					Span:    p.current.Span,
				}
			}
			separator = SeparatorNewline
			hasSeparator = true
		}
	}

	if !hasSeparator {
		separator = SeparatorComma
	}

	if p.check(TokenEOF) {
		return nil, &ParseError{
			Message: "unclosed object (missing `}`)",
			Span:    openBrace.Span,
		}
	}

	closeBrace, err := p.expect(TokenRBrace)
	if err != nil {
		return nil, err
	}
	return &Object{Entries: entries, Separator: separator, Span: Span{start, closeBrace.Span.End}}, nil
}

func (p *parser) parseSequence() (*Sequence, error) {
	openParen, err := p.expect(TokenLParen)
	if err != nil {
		return nil, err
	}
	start := openParen.Span.Start
	items := []*Value{}

	for !p.check(TokenRParen, TokenEOF) {
		item, err := p.parseValue()
		if err != nil {
			return nil, err
		}
		items = append(items, item)
	}

	if p.check(TokenEOF) {
		return nil, &ParseError{
			Message: "unclosed sequence (missing `)`)",
			Span:    openParen.Span,
		}
	}

	closeParen, err := p.expect(TokenRParen)
	if err != nil {
		return nil, err
	}
	return &Sequence{Items: items, Span: Span{start, closeParen.Span.End}}, nil
}
