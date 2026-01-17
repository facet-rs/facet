"use strict";
var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __hasOwnProp = Object.prototype.hasOwnProperty;
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};
var __copyProps = (to, from, except, desc) => {
  if (from && typeof from === "object" || typeof from === "function") {
    for (let key of __getOwnPropNames(from))
      if (!__hasOwnProp.call(to, key) && key !== except)
        __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
  }
  return to;
};
var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

// src/index.ts
var index_exports = {};
__export(index_exports, {
  parser: () => parser,
  styx: () => styx,
  styxLanguage: () => styxLanguage
});
module.exports = __toCommonJS(index_exports);

// src/syntax.grammar.ts
var import_lr = require("@lezer/lr");

// src/highlight.ts
var import_highlight = require("@lezer/highlight");
var styxHighlight = (0, import_highlight.styleTags)({
  Tag: import_highlight.tags.tagName,
  BareScalar: import_highlight.tags.string,
  QuotedScalar: import_highlight.tags.string,
  RawScalar: import_highlight.tags.special(import_highlight.tags.string),
  Heredoc: import_highlight.tags.special(import_highlight.tags.string),
  Attributes: import_highlight.tags.attributeName,
  Unit: import_highlight.tags.null,
  Comment: import_highlight.tags.lineComment,
  DocComment: import_highlight.tags.docComment,
  "( )": import_highlight.tags.paren,
  "{ }": import_highlight.tags.brace,
  ",": import_highlight.tags.separator
});

// src/syntax.grammar.ts
var parser = import_lr.LRParser.deserialize({
  version: 14,
  states: "(dQVQPOOOOQO'#C^'#C^OzQPO'#C_OOQO'#Cb'#CbOOQO'#Cd'#CdOOQO'#Ce'#CeO#ZQPO'#CnOOQO'#Cs'#CsOOQO'#Ct'#CtOOQO'#Cv'#CvO#|QPO'#ChOOQO'#Cw'#CwO$iQPO'#CaOOQO'#Ca'#CaO%cQPO'#C`O%sQPO'#DTOOQO'#DT'#DTOOQO'#C|'#C|QVQPOOOOQO'#C}'#C}OOQO-E6{-E6{OOQO'#Cp'#CpO#bQPO'#CpO%xQPO'#CoOOQO,59Y,59YO&TQPO,59YOOQO'#Cu'#CuO&YQPO'#CjOOQO'#Cj'#CjOOQO'#DO'#DOO&yQPO'#CiOOQO,59S,59SO'QQPO,59SOOQO'#Cc'#CcOOQO,58{,58{OOQO'#C{'#C{OOQO'#Cz'#CzO'VQPO'#CxOOQO'#Cx'#CxOOQO,58z,58zOOQO,59o,59oOOQO-E6z-E6zOOQO,59[,59[OOQO'#Cq'#CqO'dQPO,59ZO'kQPO,59ZOOQO1G.t1G.tOOQO'#Ck'#CkOOQO,59U,59UOOQO-E6|-E6|OOQO1G.n1G.nOOQO'#Cy'#CyOOQO,59d,59dOOQO,59k,59kO'vQPO1G.uOOQO-E6}-E6}P!lQPO'#DP",
  stateData: "(X~OvOS~OZYOaUOxPOy`OzcO{RO|SO}TO!OVO!PWO!QXO~OzcOyRXZRXaRX{RX|RX}RX!ORX!PRX!QRX~OZYOaUOzcO{RO|SO}TO!OVO!PWO!QXO~O`hO~P!lOZYOaUO{RO|SO}TO!OVO!PWO!QXO~OYoO~P#bOZYOaUO|SO}TO!OVO!PWO~OZTXaTXyTX{TX|TX}TX!OTX!PTX!QTX!RTX`TXfTX~P$TO!RsOySX`SXfSX~P#bOyxO~Of{Oy{O`cX~O`!OO~OY^XZ^Xa^X{^X|^X}^X!O^X!P^X!Q^X~P$TOY]X~P#bOY!SO~OylX`lXflX~P$TO`ca~P!lOf{Oy{O`ca~O`ci~P!lO{!Ozx}!R!P!Q}~",
  goto: "%sxPPy}!X!f!o#P#S#SPP#S#m#p#tPP#S#w#z$SP#S#S$Y$^$n$w$z$}$k%Q%W%c%iPPP%oT_ObS_ObXfU|!W!YS_ObWeU|!W!YRzf_^OUbf|!W!Y^[OUbf|!W!YSkYnRu^Rr[^ZOUbf|!W!YSjYnQq[Qt^Q!PkR!TuRpYTmYnR!QkRiUQgUV!V|!W!YQ|gR!W}TlYn^ZOUbf|!W!YSjYnRt^_]OUbf|!W!YRw^R!UuRv^QbORyb[QOUb|!W!YRdQQnYR!RnQ}gR!X}TaOb",
  nodeNames: "\u26A0 Document Comment DocComment Entry KeyExpr Tag KeyPayload QuotedScalar RawScalar ) ( Sequence SeqContent SeqItem SeqPayload } { Object ObjContent ObjItem ObjSep , Unit Attributes SeqAtom BareScalar KeyAtom ValueExpr ValuePayload ValueAtom Heredoc",
  maxTerm: 49,
  nodeProps: [
    ["openedBy", 10, "(", 16, "{"],
    ["closedBy", 11, ")", 17, "}"]
  ],
  propSources: [styxHighlight],
  skippedNodes: [0],
  repeatNodeCount: 4,
  tokenData: "Ec~RmOX!|XY(OYZ(ZZ]!|]^(`^p!|pq(Oqr!|rs(fsx!|xy)myz)rz|!||})w}!P!|!P!Q)|!Q!^!|!^!_>r!a!b!|!b!cBo!c#f!|#f#gCc#g#o!|#o#pEX#p#q!|#q#rE^#r;'S!|;'S;=`'x<%lO!|~#R]!Q~OX!|Z]!|^p!|qr!|sx!|z|!|}!`!|!`!a#z!a#o!|#p#q!|#r;'S!|;'S;=`'x<%lO!|~#}ZOX$pZ]$p^p$pqr$psx$pz|$p}#o$p#p#q$p#r;'S$p;'S;=`'r<%lO$p~$u]!P~OX$pXY%nZ]$p^p$ppq%nqr$psx$pz|$p}#o$p#p#q$p#r;'S$p;'S;=`'r<%lO$p~%q_OX&pXY%nZ]&p^p&ppq%nqr&psx&pz|&p}!_&p!a!b&p!c#o&p#p#q&p#r;'S&p;'S;=`'l<%lO&p~&s]OX&pZ]&p^p&pqr&psx&pz|&p}!`&p!`!a#z!a#o&p#p#q&p#r;'S&p;'S;=`'l<%lO&p~'oP;=`<%l&p~'uP;=`<%l$p~'{P;=`<%l!|~(TQv~XY(Opq(O~(`Oy~~(cPYZ(Z~(iXOY(fZ](f^r(frs)Us#O(f#O#P)Z#P;'S(f;'S;=`)g<%lO(f~)ZO|~~)^RO;'S(f;'S;=`)g<%lO(f~)jP;=`<%l(f~)rOZ~~)wOY~~)|Of~~*R_!Q~OX!|Z]!|^p!|qr!|sx!|z|!|}!P!|!P!Q+Q!Q!`!|!`!a#z!a#o!|#p#q!|#r;'S!|;'S;=`'x<%lO!|~+Vf!Q~OX,kXY.WZ],k^p,kpq.Wqr,krs.Wsx,kxz.Wz|,k|}.W}!P,k!P!Q5]!Q!`,k!`!a.u!a#o,k#o#p.W#p#q,k#q#r.W#r;'S,k;'S;=`5V<%lO,k~,rfx~!Q~OX,kXY.WZ],k^p,kpq.Wqr,krs.Wsx,kxz.Wz|,k|}.W}!`,k!`!a.u!a#Q,k#Q#R!|#R#o,k#o#p.W#p#q,k#q#r.W#r;'S,k;'S;=`5V<%lO,k~.]Ux~OY.WZ].W^#Q.W#R;'S.W;'S;=`.o<%lO.W~.rP;=`<%l.W~.zdx~OX0YXY.WZ]0Y^p0Ypq.Wqr0Yrs.Wsx0Yxz.Wz|0Y|}.W}#Q0Y#Q#R$p#R#o0Y#o#p.W#p#q0Y#q#r.W#r;'S0Y;'S;=`5P<%lO0Y~0adx~!P~OX0YXY1oZ]0Y^p0Ypq1oqr0Yrs.Wsx0Yxz.Wz|0Y|}.W}#Q0Y#Q#R$p#R#o0Y#o#p.W#p#q0Y#q#r.W#r;'S0Y;'S;=`5P<%lO0Y~1thx~OX3`XY1oZ]3`^p3`pq1oqr3`rs.Wsx3`xz.Wz|3`|}.W}!_3`!_!a.W!a!b3`!b!c.W!c#Q3`#Q#R&p#R#o3`#o#p.W#p#q3`#q#r.W#r;'S3`;'S;=`4y<%lO3`~3efx~OX3`XY.WZ]3`^p3`pq.Wqr3`rs.Wsx3`xz.Wz|3`|}.W}!`3`!`!a.u!a#Q3`#Q#R&p#R#o3`#o#p.W#p#q3`#q#r.W#r;'S3`;'S;=`4y<%lO3`~4|P;=`<%l3`~5SP;=`<%l0Y~5YP;=`<%l,k~5bh!Q~OX5]XY6|YZ7iZ]5]]^7n^p5]pq6|qr5]rs6|sx5]xz6|z|5]|}6|}!`5]!`!a7z!a#Q5]#Q#R!|#R#o5]#o#p6|#p#q5]#q#r6|#r;'S5];'S;=`>l<%lO5]~7PWOY6|YZ7iZ]6|]^7n^#Q6|#R;'S6|;'S;=`7t<%lO6|~7nOz~~7qPYZ7i~7wP;=`<%l6|~7}fOX9cXY6|YZ7iZ]9c]^7n^p9cpq6|qr9crs6|sx9cxz6|z|9c|}6|}#Q9c#Q#R$p#R#o9c#o#p6|#p#q9c#q#r6|#r;'S9c;'S;=`>f<%lO9c~9hf!P~OX9cXY:|YZ7iZ]9c]^7n^p9cpq:|qr9crs6|sx9cxz6|z|9c|}6|}#Q9c#Q#R$p#R#o9c#o#p6|#p#q9c#q#r6|#r;'S9c;'S;=`>f<%lO9c~;PjOX<qXY:|YZ7iZ]<q]^7n^p<qpq:|qr<qrs6|sx<qxz6|z|<q|}6|}!_<q!_!a6|!a!b<q!b!c6|!c#Q<q#Q#R&p#R#o<q#o#p6|#p#q<q#q#r6|#r;'S<q;'S;=`>`<%lO<q~<thOX<qXY6|YZ7iZ]<q]^7n^p<qpq6|qr<qrs6|sx<qxz6|z|<q|}6|}!`<q!`!a7z!a#Q<q#Q#R&p#R#o<q#o#p6|#p#q<q#q#r6|#r;'S<q;'S;=`>`<%lO<q~>cP;=`<%l<q~>iP;=`<%l9c~>oP;=`<%l5]~>w_!Q~OX!|Z]!|^p!|qr!|sx!|z|!|}!^!|!^!_?v!_!`!|!`!a#z!a#o!|#p#q!|#r;'S!|;'S;=`'x<%lO!|~?{_!Q~OX!|Z]!|^p!|qr!|sx!|z|!|}!`!|!`!a#z!a!c!|!c!}@z!}#o!|#p#q!|#r;'S!|;'S;=`'x<%lO!|~ARd!R~!Q~OX!|Z]!|^p!|qr!|sx!|z|!||}Ba}!Q!|!Q![@z![!`!|!`!a#z!a!c!|!c!}@z!}#R!|#R#S@z#S#o!|#p#q!|#r;'S!|;'S;=`'x<%lO!|~BdP#T#oBg~BlP!R~#T#oBg~BtR!O~!c!}B}#R#SB}#T#oB}~CST{~}!OB}!Q![B}!c!}B}#R#SB}#T#oB}~Ch_!Q~OX!|Z]!|^p!|qr!|rsDgstCctx!|z|!|}!`!|!`!a#z!a#o!|#p#q!|#r;'S!|;'S;=`'x<%lO!|~DjTOrDgrsDys;'SDg;'S;=`ER<%lODg~EOP}~stDy~EUP;=`<%lDg~E^Oa~~EcO`~",
  tokenizers: [0],
  topRules: { "Document": [0, 1] },
  tokenPrec: 321
});

// src/index.ts
var import_language = require("@codemirror/language");
var import_autocomplete = require("@codemirror/autocomplete");
var import_highlight3 = require("@lezer/highlight");
var styxLanguage = import_language.LRLanguage.define({
  name: "styx",
  parser,
  languageData: {
    commentTokens: { line: "//" },
    closeBrackets: { brackets: ["(", "{", '"'] }
  }
});
var builtinTags = [
  "@string",
  "@int",
  "@float",
  "@bool",
  "@null",
  "@object",
  "@array",
  "@optional",
  "@required",
  "@default",
  "@enum",
  "@pattern",
  "@min",
  "@max",
  "@minLength",
  "@maxLength"
].map((label) => ({ label, type: "keyword" }));
var styxCompletion = styxLanguage.data.of({
  autocomplete: (0, import_autocomplete.completeFromList)(builtinTags)
});
var styxHighlightStyle = import_language.HighlightStyle.define([
  { tag: import_highlight3.tags.lineComment, color: "#6a9955" },
  { tag: import_highlight3.tags.docComment, color: "#6a9955", fontStyle: "italic" },
  { tag: import_highlight3.tags.string, color: "#ce9178" },
  { tag: import_highlight3.tags.special(import_highlight3.tags.string), color: "#d7ba7d" },
  { tag: import_highlight3.tags.tagName, color: "#569cd6" },
  { tag: import_highlight3.tags.attributeName, color: "#9cdcfe" },
  { tag: import_highlight3.tags.null, color: "#569cd6" },
  { tag: import_highlight3.tags.paren, color: "#ffd700" },
  { tag: import_highlight3.tags.brace, color: "#da70d6" },
  { tag: import_highlight3.tags.separator, color: "#d4d4d4" }
]);
var styxHighlightingExt = (0, import_language.syntaxHighlighting)(styxHighlightStyle);
function styx() {
  return new import_language.LanguageSupport(styxLanguage, [styxCompletion, styxHighlightingExt]);
}
// Annotate the CommonJS export names for ESM import in node:
0 && (module.exports = {
  parser,
  styx,
  styxLanguage
});
