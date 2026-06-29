import grammarJs from "./nginx/grammar.js?raw";
import highlights from "./nginx/highlights.scm?raw";
import injections from "./nginx/injections.scm?raw";
import sample from "./nginx/nginx.conf?raw";
import errorSample from "./nginx/nginx-errors.conf?raw";
import type { VendoredGrammar } from "./index";

// tree-sitter-nginx (from arborium). nginx-errors.conf is a deliberately-broken config.
export const nginx: VendoredGrammar = {
  id: "nginx",
  label: "nginx",
  files: [
    { path: "nginx/grammar.js", text: grammarJs },
    { path: "nginx/queries/highlights.scm", text: highlights },
    { path: "nginx/queries/injections.scm", text: injections },
    { path: "nginx/samples/nginx.conf", text: sample },
    { path: "nginx/samples/nginx-errors.conf", text: errorSample },
  ],
};
