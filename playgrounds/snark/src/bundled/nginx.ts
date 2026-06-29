// The nginx grammar bundle, shipped so the playground opens on a real grammar
// (tree-sitter-nginx from arborium) instead of a toy. Files are imported raw and
// run through the same normalization the upload path uses.
import grammarJs from "./nginx/grammar.js?raw";
import highlights from "./nginx/highlights.scm?raw";
import injections from "./nginx/injections.scm?raw";
import sample from "./nginx/nginx.conf?raw";
import { normalizeBundleFiles, sortedFiles, type DslBundleFile } from "../treeSitterDsl";

const rawFiles: DslBundleFile[] = [
  { path: "grammar.js", text: grammarJs },
  { path: "queries/highlights.scm", text: highlights },
  { path: "queries/injections.scm", text: injections },
  { path: "samples/nginx.conf", text: sample },
];

export const nginxDefaultFiles: DslBundleFile[] = sortedFiles(normalizeBundleFiles(rawFiles));
