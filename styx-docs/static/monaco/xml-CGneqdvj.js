import { m as r } from "./main-B6eUmL6x.js";
var m = Object.defineProperty, c = Object.getOwnPropertyDescriptor, l = Object.getOwnPropertyNames, d = Object.prototype.hasOwnProperty, p = (t, e, o, i) => {
  if (e && typeof e == "object" || typeof e == "function")
    for (let n of l(e))
      !d.call(t, n) && n !== o && m(t, n, { get: () => e[n], enumerable: !(i = c(e, n)) || i.enumerable });
  return t;
}, s = (t, e, o) => (p(t, e, "default"), o), a = {};
s(a, r);
var g = {
  comments: {
    blockComment: ["<!--", "-->"]
  },
  brackets: [["<", ">"]],
  autoClosingPairs: [
    { open: "<", close: ">" },
    { open: "'", close: "'" },
    { open: '"', close: '"' }
  ],
  surroundingPairs: [
    { open: "<", close: ">" },
    { open: "'", close: "'" },
    { open: '"', close: '"' }
  ],
  onEnterRules: [
    {
      beforeText: new RegExp("<([_:\\w][_:\\w-.\\d]*)([^/>]*(?!/)>)[^<]*$", "i"),
      afterText: /^<\/([_:\w][_:\w-.\d]*)\s*>$/i,
      action: {
        indentAction: a.languages.IndentAction.IndentOutdent
      }
    },
    {
      beforeText: new RegExp("<(\\w[\\w\\d]*)([^/>]*(?!/)>)[^<]*$", "i"),
      action: { indentAction: a.languages.IndentAction.Indent }
    }
  ]
}, k = {
  defaultToken: "",
  tokenPostfix: ".xml",
  ignoreCase: !0,
  // Useful regular expressions
  qualifiedName: /(?:[\w\.\-]+:)?[\w\.\-]+/,
  tokenizer: {
    root: [
      [/[^<&]+/, ""],
      { include: "@whitespace" },
      // Standard opening tag
      [/(<)(@qualifiedName)/, [{ token: "delimiter" }, { token: "tag", next: "@tag" }]],
      // Standard closing tag
      [
        /(<\/)(@qualifiedName)(\s*)(>)/,
        [{ token: "delimiter" }, { token: "tag" }, "", { token: "delimiter" }]
      ],
      // Meta tags - instruction
      [/(<\?)(@qualifiedName)/, [{ token: "delimiter" }, { token: "metatag", next: "@tag" }]],
      // Meta tags - declaration
      [/(<\!)(@qualifiedName)/, [{ token: "delimiter" }, { token: "metatag", next: "@tag" }]],
      // CDATA
      [/<\!\[CDATA\[/, { token: "delimiter.cdata", next: "@cdata" }],
      [/&\w+;/, "string.escape"]
    ],
    cdata: [
      [/[^\]]+/, ""],
      [/\]\]>/, { token: "delimiter.cdata", next: "@pop" }],
      [/\]/, ""]
    ],
    tag: [
      [/[ \t\r\n]+/, ""],
      [/(@qualifiedName)(\s*=\s*)("[^"]*"|'[^']*')/, ["attribute.name", "", "attribute.value"]],
      [
        /(@qualifiedName)(\s*=\s*)("[^">?\/]*|'[^'>?\/]*)(?=[\?\/]\>)/,
        ["attribute.name", "", "attribute.value"]
      ],
      [/(@qualifiedName)(\s*=\s*)("[^">]*|'[^'>]*)/, ["attribute.name", "", "attribute.value"]],
      [/@qualifiedName/, "attribute.name"],
      [/\?>/, { token: "delimiter", next: "@pop" }],
      [/(\/)(>)/, [{ token: "tag" }, { token: "delimiter", next: "@pop" }]],
      [/>/, { token: "delimiter", next: "@pop" }]
    ],
    whitespace: [
      [/[ \t\r\n]+/, ""],
      [/<!--/, { token: "comment", next: "@comment" }]
    ],
    comment: [
      [/[^<\-]+/, "comment.content"],
      [/-->/, { token: "comment", next: "@pop" }],
      [/<!--/, "comment.content.invalid"],
      [/[<\-]/, "comment.content"]
    ]
  }
};
export {
  g as conf,
  k as language
};
