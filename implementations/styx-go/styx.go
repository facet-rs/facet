// Package styx provides a parser for the Styx configuration language.
package styx

import "fmt"

// Span represents a byte range in the source.
type Span struct {
	Start int
	End   int
}

// ParseError represents a parse error with location information.
type ParseError struct {
	Message string
	Span    Span
}

func (e *ParseError) Error() string {
	return fmt.Sprintf("parse error at %d-%d: %s", e.Span.Start, e.Span.End, e.Message)
}

// ScalarKind represents the kind of scalar value.
type ScalarKind int

const (
	ScalarBare ScalarKind = iota
	ScalarQuoted
	ScalarRaw
	ScalarHeredoc
)

func (k ScalarKind) String() string {
	switch k {
	case ScalarBare:
		return "bare"
	case ScalarQuoted:
		return "quoted"
	case ScalarRaw:
		return "raw"
	case ScalarHeredoc:
		return "heredoc"
	default:
		return "unknown"
	}
}

// Separator represents the separator style in an object.
type Separator int

const (
	SeparatorComma Separator = iota
	SeparatorNewline
)

func (s Separator) String() string {
	switch s {
	case SeparatorComma:
		return "comma"
	case SeparatorNewline:
		return "newline"
	default:
		return "unknown"
	}
}

// Scalar represents a scalar value.
type Scalar struct {
	Text string
	Kind ScalarKind
	Span Span
}

// Tag represents a tag annotation.
type Tag struct {
	Name string
	Span Span
}

// Entry represents a key-value entry in an object.
type Entry struct {
	Key   *Value
	Value *Value
}

// Sequence represents a sequence of values.
type Sequence struct {
	Items []*Value
	Span  Span
}

// Object represents an object with key-value entries.
type Object struct {
	Entries   []*Entry
	Separator Separator
	Span      Span
}

// PayloadKind identifies the type of payload in a Value.
type PayloadKind int

const (
	PayloadNone PayloadKind = iota
	PayloadScalar
	PayloadSequence
	PayloadObject
)

// Value represents a Styx value - can have a tag and/or payload.
type Value struct {
	Span        Span
	Tag         *Tag
	PayloadKind PayloadKind
	Scalar      *Scalar
	Sequence    *Sequence
	Object      *Object
}

// IsUnit returns true if this is a unit value (no tag, no payload).
func (v *Value) IsUnit() bool {
	return v.Tag == nil && v.PayloadKind == PayloadNone
}

// Document represents a parsed Styx document.
type Document struct {
	Entries []*Entry
	Span    Span
}

// Parse parses a Styx document from the source string.
func Parse(source string) (*Document, error) {
	p := newParser(source)
	return p.parse()
}
