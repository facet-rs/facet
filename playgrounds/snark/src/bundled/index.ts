// Registry of grammars vendored into the playground.
//
// Every file under bundled/<id>/ is imported as raw text and grouped by its top-level
// folder, which becomes a selectable grammar root (id == path prefix). Vendoring a new
// grammar is therefore just: drop a folder `bundled/<id>/` containing grammar.js,
// queries/highlights.scm, and samples/* — no code changes here.
import { normalizeBundleFiles, sortedFiles, type DslBundleFile } from "../treeSitterDsl";

export type VendoredGrammar = {
  /** Equals the grammar-root id, i.e. the path prefix of its files. */
  id: string;
  label: string;
  files: DslBundleFile[];
};

// Eager raw import of every vendored asset. Keys look like "./gingembre/grammar.js".
const rawAssets = import.meta.glob("./*/**/*", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const allFiles: DslBundleFile[] = Object.entries(rawAssets).map(([key, text]) => ({
  path: key.replace(/^\.\//, ""),
  text,
}));

const grammarIds = [...new Set(allFiles.map((file) => file.path.split("/")[0]))].sort();

export const vendoredGrammars: VendoredGrammar[] = grammarIds.map((id) => ({
  id,
  label: id,
  files: allFiles.filter((file) => file.path.startsWith(`${id}/`)),
}));

export const vendoredFiles: DslBundleFile[] = sortedFiles(normalizeBundleFiles(allFiles));

/** Grammar selected on first open. */
export const defaultVendoredRootId = grammarIds.includes("gingembre")
  ? "gingembre"
  : (grammarIds[0] ?? "");
