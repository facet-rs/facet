import { styleTags, tags as t } from "@lezer/highlight";

export const styxHighlight = styleTags({
  Tag: t.tagName,
  BareScalar: t.string,
  QuotedScalar: t.string,
  RawScalar: t.special(t.string),
  Heredoc: t.special(t.string),
  Attributes: t.attributeName,
  Unit: t.null,
  Comment: t.lineComment,
  DocComment: t.docComment,
  "( )": t.paren,
  "{ }": t.brace,
  ",": t.separator,
});
