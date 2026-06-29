// Registry of grammars vendored into the playground. Each grammar's files are
// prefixed with its id (e.g. `gingembre/grammar.js`), so they surface as separate
// selectable grammar roots in the toolbar. Adding a grammar = drop a folder under
// bundled/<id>/, write a small module like ./gingembre, and list it here.
import { gingembre } from "./gingembre";
import { nginx } from "./nginx";
import { normalizeBundleFiles, sortedFiles, type DslBundleFile } from "../treeSitterDsl";

export type VendoredGrammar = {
  /** Must equal the grammar-root id, i.e. the path prefix of its files. */
  id: string;
  label: string;
  files: DslBundleFile[];
};

export const vendoredGrammars: VendoredGrammar[] = [gingembre, nginx];

export const vendoredFiles: DslBundleFile[] = sortedFiles(
  normalizeBundleFiles(vendoredGrammars.flatMap((grammar) => grammar.files)),
);

/** Grammar selected on first open. */
export const defaultVendoredRootId = gingembre.id;
