package styx

import (
	"strings"
)

// pathValueKind tracks whether a path leads to an object or terminal value.
type pathValueKind int

const (
	pathValueObject pathValueKind = iota
	pathValueTerminal
)

// pathState tracks dotted path state for validation.
type pathState struct {
	currentPath   []string
	closedPaths   map[string]bool // key is joined path
	assignedPaths map[string]struct {
		kind pathValueKind
		span Span
	}
}

func newPathState() *pathState {
	return &pathState{
		closedPaths: make(map[string]bool),
		assignedPaths: make(map[string]struct {
			kind pathValueKind
			span Span
		}),
	}
}

func joinPath(segments []string) string {
	return strings.Join(segments, ".")
}

// checkAndUpdate validates a path and updates the state.
// Returns an error if the path is invalid.
func (ps *pathState) checkAndUpdate(path []string, span Span, kind pathValueKind) error {
	pathKey := joinPath(path)

	// 1. Check for duplicate (exact same path)
	if _, exists := ps.assignedPaths[pathKey]; exists {
		return &ParseError{Message: "duplicate key", Span: span}
	}

	// 2. Check if any proper prefix is closed or has a terminal value
	for i := 1; i < len(path); i++ {
		prefix := path[:i]
		prefixKey := joinPath(prefix)
		if ps.closedPaths[prefixKey] {
			return &ParseError{
				Message: "cannot reopen path `" + prefixKey + "` after sibling appeared",
				Span:    span,
			}
		}
		if assigned, exists := ps.assignedPaths[prefixKey]; exists && assigned.kind == pathValueTerminal {
			return &ParseError{
				Message: "cannot nest into `" + prefixKey + "` which has a terminal value",
				Span:    span,
			}
		}
	}

	// 3. Find common prefix length with current path
	commonLen := 0
	for i := 0; i < len(ps.currentPath) && i < len(path); i++ {
		if ps.currentPath[i] == path[i] {
			commonLen++
		} else {
			break
		}
	}

	// 4. Close paths beyond the common prefix
	for i := commonLen; i < len(ps.currentPath); i++ {
		closed := joinPath(ps.currentPath[:i+1])
		ps.closedPaths[closed] = true
	}

	// 5. Record intermediate path segments as objects (if not already assigned)
	for i := 1; i < len(path); i++ {
		prefix := path[:i]
		prefixKey := joinPath(prefix)
		if _, exists := ps.assignedPaths[prefixKey]; !exists {
			ps.assignedPaths[prefixKey] = struct {
				kind pathValueKind
				span Span
			}{pathValueObject, span}
		}
	}

	// 6. Update assigned paths and current path
	ps.assignedPaths[pathKey] = struct {
		kind pathValueKind
		span Span
	}{kind, span}
	ps.currentPath = path

	return nil
}

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
	ps := newPathState()

	for !p.check(TokenEOF) {
		if p.err != nil {
			return nil, p.err
		}
		entry, err := p.parseEntryWithPathCheck(ps)
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

func (p *parser) parseEntryWithPathCheck(ps *pathState) (*Entry, error) {
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
			return p.expandDottedPathWithState(text, key.Span, ps)
		}
	}

	if err := p.validateKey(key); err != nil {
		return nil, err
	}

	// Get key text for path tracking
	keyText := p.getKeyText(key)

	// Check for implicit unit
	if p.current.HadNewlineBefore || p.check(TokenEOF, TokenRBrace) {
		// Validate path
		if keyText != "" {
			if err := ps.checkAndUpdate([]string{keyText}, key.Span, pathValueTerminal); err != nil {
				return nil, err
			}
		}
		return &Entry{Key: key, Value: &Value{Span: key.Span}}, nil
	}

	value, err := p.parseValue()
	if err != nil {
		return nil, err
	}

	// Determine value kind and validate path
	if keyText != "" {
		kind := pathValueTerminal
		if value.PayloadKind == PayloadObject {
			kind = pathValueObject
		}
		if err := ps.checkAndUpdate([]string{keyText}, key.Span, kind); err != nil {
			return nil, err
		}
	}

	return &Entry{Key: key, Value: value}, nil
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

func (p *parser) expandDottedPathWithState(pathText string, span Span, ps *pathState) (*Entry, error) {
	segments := strings.Split(pathText, ".")

	for _, s := range segments {
		if s == "" {
			return nil, &ParseError{Message: "invalid key", Span: span}
		}
	}

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

	// Determine value kind for path tracking
	kind := pathValueTerminal
	if value.PayloadKind == PayloadObject {
		kind = pathValueObject
	}

	// Validate path with state
	if err := ps.checkAndUpdate(segments, span, kind); err != nil {
		return nil, err
	}

	// Build nested structure from inside out
	// Object spans start at the PREVIOUS segment's position (i-1)
	lastKeyEnd := segmentSpans[len(segments)-1].End
	result := value
	for i := len(segments) - 1; i > 0; i-- {
		segSpan := segmentSpans[i]
		segmentKey := &Value{
			Span:        segSpan,
			PayloadKind: PayloadScalar,
			Scalar:      &Scalar{Text: segments[i], Kind: ScalarBare, Span: segSpan},
		}
		// Object span starts at the previous segment's position
		objStart := segmentSpans[i-1].Start
		objSpan := Span{objStart, lastKeyEnd}
		result = &Value{
			Span:        objSpan,
			PayloadKind: PayloadObject,
			Object: &Object{
				Entries:   []*Entry{{Key: segmentKey, Value: result}},
				Separator: SeparatorNewline,
				Span:      objSpan,
			},
		}
	}

	firstSpan := segmentSpans[0]
	outerKey := &Value{
		Span:        firstSpan,
		PayloadKind: PayloadScalar,
		Scalar:      &Scalar{Text: segments[0], Kind: ScalarBare, Span: firstSpan},
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
			gtToken := p.advance() // consume >
			afterGT := p.current

			// Check if > is followed by something that can't be an attribute value
			if afterGT.HadNewlineBefore || afterGT.HadWhitespaceBefore || p.check(TokenEOF, TokenRBrace, TokenRParen, TokenComma) {
				// Error: trailing > without a value
				return nil, &ParseError{
					Message: "expected a value",
					Span:    gtToken.Span,
				}
			}
			// Valid attribute - parse value (we already consumed >)
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

		p.advance()            // consume key
		gtToken := p.advance() // consume >

		// Check if > is followed by something that can't be an attribute value
		afterGT := p.current
		if afterGT.HadNewlineBefore || afterGT.HadWhitespaceBefore || p.check(TokenEOF, TokenRBrace, TokenRParen, TokenComma) {
			return nil, &ParseError{
				Message: "expected a value",
				Span:    gtToken.Span,
			}
		}

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
		// Check for comma - not allowed in sequences
		if p.check(TokenComma) {
			return nil, &ParseError{
				Message: "unexpected `,` in sequence (sequences are whitespace-separated, not comma-separated)",
				Span:    p.current.Span,
			}
		}
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
