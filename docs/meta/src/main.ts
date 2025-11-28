/**
 * Custom highlight.js bundle for facet rustdoc documentation
 *
 * This bundles highlight.js core with only the languages we need,
 * plus custom KDL and TOML definitions.
 */

import hljs from "highlight.js/lib/core";

// Import built-in languages we need
import json from "highlight.js/lib/languages/json";
import yaml from "highlight.js/lib/languages/yaml";
import xml from "highlight.js/lib/languages/xml";
import bash from "highlight.js/lib/languages/bash";
import plaintext from "highlight.js/lib/languages/plaintext";

// Import custom language definitions
import kdl from "./languages/kdl";
import toml from "./languages/toml";

// Register built-in languages
hljs.registerLanguage("json", json);
hljs.registerLanguage("yaml", yaml);
hljs.registerLanguage("xml", xml);
hljs.registerLanguage("html", xml); // HTML uses XML highlighter
hljs.registerLanguage("bash", bash);
hljs.registerLanguage("shell", bash);
hljs.registerLanguage("sh", bash);
hljs.registerLanguage("plaintext", plaintext);
hljs.registerLanguage("text", plaintext);

// Register custom languages
hljs.registerLanguage("kdl", kdl);
hljs.registerLanguage("toml", toml);

// Import Tokyo Night styles (will be inlined by Vite)
import "highlight.js/styles/tokyo-night-dark.css";

/**
 * Initialize syntax highlighting for rustdoc pages
 */
function initRustdocHighlighting(): void {
  // Check if we're in a rustdoc page
  const generatorMeta = document.querySelector('meta[name="generator"]');
  const isRustdoc = generatorMeta?.getAttribute("content") === "rustdoc";

  if (!isRustdoc) {
    console.log("[facet-hljs] Not a rustdoc page, skipping initialization");
    return;
  }

  console.log("[facet-hljs] Initializing syntax highlighting for rustdoc");

  // Languages to highlight (rustdoc doesn't handle these natively)
  const customLanguageSelectors = [
    ".language-kdl",
    ".language-json",
    ".language-yaml",
    ".language-toml",
    ".language-xml",
    ".language-html",
    ".language-bash",
    ".language-shell",
    ".language-sh",
    ".language-text",
    ".language-plaintext",
  ];

  // Highlight code blocks with our custom languages
  for (const selector of customLanguageSelectors) {
    const codeBlocks = document.querySelectorAll<HTMLElement>(selector);
    for (const codeBlock of codeBlocks) {
      hljs.highlightElement(codeBlock);
    }
  }

  // Ensure highlighted blocks have the rust class for rustdoc styling
  const highlightedBlocks = document.querySelectorAll<HTMLElement>(".hljs");
  for (const codeBlock of highlightedBlocks) {
    codeBlock.classList.add("rust");
  }

  // Map highlight.js classes to rustdoc classes for theme compatibility
  const classMap: [string, string][] = [
    ["hljs-comment", "comment"],
    ["hljs-number", "number"],
    ["hljs-keyword", "kw"],
    ["hljs-built_in", "prelude-ty"],
    ["hljs-string", "string"],
    ["hljs-title", "fn"],
    ["hljs-type", "type"],
    ["hljs-attr", "attribute"],
    ["hljs-literal", "bool-val"],
    ["hljs-section", "attribute"],
    ["hljs-variable", "self"],
    ["hljs-name", "kw"],
    ["hljs-tag", "macro"],
  ];

  for (const [hljsClass, rustdocClass] of classMap) {
    const elements = document.querySelectorAll<HTMLElement>(`.${hljsClass}`);
    for (const element of elements) {
      element.classList.remove(hljsClass);
      element.classList.add(rustdocClass);
    }
  }

  console.log("[facet-hljs] Syntax highlighting complete");
}

// Run on DOMContentLoaded
if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", initRustdocHighlighting);
} else {
  initRustdocHighlighting();
}

// Export hljs for manual use if needed
export { hljs };
