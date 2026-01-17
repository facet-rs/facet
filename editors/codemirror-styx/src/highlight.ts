import { styleTags, tags as t } from "@lezer/highlight";

export const styxHighlight = styleTags({
  // Keys (property names) - first atom in an Entry
  "KeyAtom/BareScalar": t.propertyName,
  "KeyAtom/QuotedScalar": t.propertyName,
  "KeyPayload/BareScalar": t.propertyName,
  "KeyPayload/QuotedScalar": t.propertyName,

  // Values - second atom in an Entry
  "ValueAtom/BareScalar": t.string,
  "ValueAtom/QuotedScalar": t.string,
  "ValuePayload/BareScalar": t.string,
  "ValuePayload/QuotedScalar": t.string,

  // Sequence items
  "SeqAtom/BareScalar": t.string,
  "SeqAtom/QuotedScalar": t.string,
  "SeqPayload/BareScalar": t.string,
  "SeqPayload/QuotedScalar": t.string,

  // Tags (@foo)
  Tag: t.tagName,

  // Raw strings and heredocs
  RawScalar: t.special(t.string),
  Heredoc: t.special(t.string),

  // Other
  Attributes: t.attributeName,
  Unit: t.null,
  Comment: t.lineComment,
  DocComment: t.docComment,
  "( )": t.paren,
  "{ }": t.brace,
  ",": t.separator,
});
