import grammarJs from "./gingembre/grammar.js?raw";
import highlights from "./gingembre/queries/highlights.scm?raw";
import blogIndex from "./gingembre/samples/blog-index.html?raw";
import blogSection from "./gingembre/samples/blog-section.html?raw";
import docsBase from "./gingembre/samples/docs-base.html?raw";
import docsPage from "./gingembre/samples/docs-page.html?raw";
import figure from "./gingembre/samples/figure.html?raw";
import showcaseMacros from "./gingembre/samples/showcase-macros.html?raw";
import type { VendoredGrammar } from "./index";

// gingembre — the Jinja-like template language used by dodeca. The grammar mirrors
// gingembre-syntax/{lexer,parser}.rs and parses the whole dodeca template corpus.
export const gingembre: VendoredGrammar = {
  id: "gingembre",
  label: "gingembre",
  files: [
    { path: "gingembre/grammar.js", text: grammarJs },
    { path: "gingembre/queries/highlights.scm", text: highlights },
    { path: "gingembre/samples/blog-index.html", text: blogIndex },
    { path: "gingembre/samples/blog-section.html", text: blogSection },
    { path: "gingembre/samples/docs-base.html", text: docsBase },
    { path: "gingembre/samples/docs-page.html", text: docsPage },
    { path: "gingembre/samples/figure.html", text: figure },
    { path: "gingembre/samples/showcase-macros.html", text: showcaseMacros },
  ],
};
