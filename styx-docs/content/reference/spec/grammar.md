+++
title = "Grammar"
weight = 3
slug = "grammar"
insert_anchor_links = "heading"
+++

Visual grammar reference for Styx. See [Parser](@/spec/parser.md) for normative rules.

**Document:**

![Document](/grammar/Document.svg)

```
Document ::= Entry*
```

**Entry:**

![Entry](/grammar/Entry.svg)

```
Entry    ::= DocComment? Key Value?
```

referenced by:

* CommaSeparated
* Document
* NewlineSeparated

**Key:**

![Key](/grammar/Key.svg)

```
Key      ::= Scalar
           | Sequence
           | '@'
           | Tag
```

referenced by:

* Entry

**Value:**

![Value](/grammar/Value.svg)

```
Value    ::= Scalar
           | Sequence
           | Object
           | '@'
           | Tag
           | Attributes
```

referenced by:

* Entry

**DocComment:**

![DocComment](/grammar/DocComment.svg)

```
DocComment
         ::= ( '///' NonNewline* Newline )+
```

referenced by:

* Entry

**Atom:**

![Atom](/grammar/Atom.svg)

```
Atom     ::= Scalar
           | Sequence
           | Object
           | '@'
           | Tag
           | Attributes
```

referenced by:

* Sequence

**Scalar:**

![Scalar](/grammar/Scalar.svg)

```
Scalar   ::= BareScalar
           | QuotedScalar
           | RawScalar
           | HeredocScalar
```

referenced by:

* Atom
* Key
* Value

**BareScalar:**

![BareScalar](/grammar/BareScalar.svg)

```
BareScalar
         ::= BareChar+
```

referenced by:

* Attribute
* AttributeValue
* Scalar

**BareChar:**

![BareChar](/grammar/BareChar.svg)

```
BareChar ::= [^{}(),"=@#x20#x09#x0A#x0D]
```

referenced by:

* BareScalar

**QuotedScalar:**

![QuotedScalar](/grammar/QuotedScalar.svg)

```
QuotedScalar
         ::= '"' QuotedChar* '"'
```

referenced by:

* AttributeValue
* Scalar
* TagPayload

**QuotedChar:**

![QuotedChar](/grammar/QuotedChar.svg)

```
QuotedChar
         ::= EscapeSeq
           | [^"\]
```

referenced by:

* QuotedScalar

**EscapeSeq:**

![EscapeSeq](/grammar/EscapeSeq.svg)

```
EscapeSeq
         ::= '\' ( [\"nrt] | 'u' HexDigit HexDigit HexDigit HexDigit | 'u{' HexDigit+ '}' )
```

referenced by:

* QuotedChar

**HexDigit:**

![HexDigit](/grammar/HexDigit.svg)

```
HexDigit ::= [0-9A-Fa-f]
```

referenced by:

* EscapeSeq

**RawScalar:**

![RawScalar](/grammar/RawScalar.svg)

```
RawScalar
         ::= 'r' '#'* '"' RawChar* '"' '#'*
```

referenced by:

* Scalar
* TagPayload

**RawChar:**

![RawChar](/grammar/RawChar.svg)

```
RawChar  ::= [^"]+
```

referenced by:

* RawScalar

**HeredocScalar:**

![HeredocScalar](/grammar/HeredocScalar.svg)

```
HeredocScalar
         ::= '<<' Delimiter Newline HeredocLine* Delimiter
```

referenced by:

* Scalar
* TagPayload

**Delimiter:**

![Delimiter](/grammar/Delimiter.svg)

```
Delimiter
         ::= [A-Z] [A-Z0-9_]*
```

referenced by:

* HeredocScalar

**HeredocLine:**

![HeredocLine](/grammar/HeredocLine.svg)

```
HeredocLine
         ::= NonNewline* Newline
```

referenced by:

* HeredocScalar

**Tag:**

![Tag](/grammar/Tag.svg)

```
Tag      ::= '@' TagName TagPayload?
```

referenced by:

* Atom
* Key
* Value

**TagName:**

![TagName](/grammar/TagName.svg)

```
TagName  ::= [A-Za-z_] [A-Za-z0-9_.#x2D]*
```

referenced by:

* Tag

**TagPayload:**

![TagPayload](/grammar/TagPayload.svg)

```
TagPayload
         ::= Object
           | Sequence
           | QuotedScalar
           | RawScalar
           | HeredocScalar
           | '@'
```

referenced by:

* Tag

**Sequence:**

![Sequence](/grammar/Sequence.svg)

```
Sequence ::= '(' WS* ( Atom ( WS+ Atom )* )? WS* ')'
```

referenced by:

* Atom
* AttributeValue
* Key
* TagPayload
* Value

**Object:**

![Object](/grammar/Object.svg)

```
Object   ::= '{' ObjectBody '}'
```

referenced by:

* Atom
* AttributeValue
* TagPayload
* Value

**ObjectBody:**

![ObjectBody](/grammar/ObjectBody.svg)

```
ObjectBody
         ::= NewlineSeparated
           | CommaSeparated
           | WS*
```

referenced by:

* Object

**NewlineSeparated:**

![NewlineSeparated](/grammar/NewlineSeparated.svg)

```
NewlineSeparated
         ::= WS* Entry ( Newline+ Entry )* WS*
```

referenced by:

* ObjectBody

**CommaSeparated:**

![CommaSeparated](/grammar/CommaSeparated.svg)

```
CommaSeparated
         ::= WS* Entry ( ',' Entry )* WS*
```

referenced by:

* ObjectBody

**Attributes:**

![Attributes](/grammar/Attributes.svg)

```
Attributes
         ::= Attribute+
```

referenced by:

* Atom
* Value

**Attribute:**

![Attribute](/grammar/Attribute.svg)

```
Attribute
         ::= BareScalar '=' AttributeValue
```

referenced by:

* Attributes

**AttributeValue:**

![AttributeValue](/grammar/AttributeValue.svg)

```
AttributeValue
         ::= BareScalar
           | QuotedScalar
           | Sequence
           | Object
```

referenced by:

* Attribute

**LineComment:**

![LineComment](/grammar/LineComment.svg)

```
LineComment
         ::= '//' NonNewline*
```

referenced by:

* WS

**WS:**

![WS](/grammar/WS.svg)

```
WS       ::= [#x20#x09#x0A#x0D]
           | LineComment
```

referenced by:

* CommaSeparated
* NewlineSeparated
* ObjectBody
* Sequence

**Newline:**

![Newline](/grammar/Newline.svg)

```
Newline  ::= #x0D? #x0A
```

referenced by:

* DocComment
* HeredocLine
* HeredocScalar
* NewlineSeparated

**NonNewline:**

![NonNewline](/grammar/NonNewline.svg)

```
NonNewline
         ::= [^#x0A#x0D]
```

referenced by:

* DocComment
* HeredocLine
* LineComment

## 
![rr-2.6](/grammar/rr-2.6.svg) <sup>generated by [RR - Railroad Diagram Generator][RR]</sup>

[RR]: https://www.bottlecaps.de/rr/ui