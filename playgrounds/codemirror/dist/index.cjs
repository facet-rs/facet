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
  Compartment: () => import_state.Compartment,
  DEFAULT_STYX_SOURCE: () => DEFAULT_STYX_SOURCE,
  EditorState: () => import_state.EditorState,
  EditorView: () => import_codemirror.EditorView,
  Prec: () => import_state.Prec,
  basicSetup: () => import_codemirror.basicSetup,
  createPlayground: () => createPlayground,
  json: () => import_lang_json.json,
  keymap: () => import_view.keymap,
  oneDark: () => import_theme_one_dark.oneDark,
  sql: () => import_lang_sql.sql,
  styx: () => import_codemirror_lang_styx.styx,
  vim: () => import_codemirror_vim.vim
});
module.exports = __toCommonJS(index_exports);
var import_codemirror = require("codemirror");
var import_state = require("@codemirror/state");
var import_view = require("@codemirror/view");
var import_theme_one_dark = require("@codemirror/theme-one-dark");
var import_lang_json = require("@codemirror/lang-json");
var import_lang_sql = require("@codemirror/lang-sql");
var import_codemirror_vim = require("@replit/codemirror-vim");
var import_codemirror_lang_styx = require("@bearcove/codemirror-lang-styx");
var DEFAULT_STYX_SOURCE = `// Welcome to the Styx Playground!
// Edit this document and see the JSON output on the right.

/// Server configuration
server {
    host localhost
    port 8080

    // Tags add semantic meaning to values
    timeout @duration(30s)

    tls {
        enabled true
        cert /etc/ssl/cert.pem
        key /etc/ssl/key.pem
    }
}

/// Database settings
database {
    url postgres://localhost/myapp
    pool_size 10
    max_connections @int(100)
}

/// Feature flags (a sequence)
features (
    dark-mode
    notifications
    @experimental(analytics)
)

/// Multi-line content with heredocs
query <<SQL,sql
SELECT id, name, email
FROM users
WHERE active = true
SQL
`;
function vimWithPrecedence(enabled) {
  return enabled ? import_state.Prec.highest((0, import_codemirror_vim.vim)()) : [];
}
function createPlayground(options) {
  const {
    wasm,
    styxContainer,
    jsonContainer,
    diagnosticsContainer,
    initialSource = DEFAULT_STYX_SOURCE,
    vimEnabled = false,
    nestedLanguages = [{ tag: "sql", language: (0, import_lang_sql.sql)() }],
    styxExtensions = [],
    jsonExtensions = [],
    onDiagnosticsChange,
    styxDebounceMs = 150,
    jsonDebounceMs = 300
  } = options;
  let styxDebounceTimer = null;
  let jsonDebounceTimer = null;
  let updatingFromStyx = false;
  let updatingFromJson = false;
  let _vimEnabled = vimEnabled;
  const vimCompartment = new import_state.Compartment();
  const styxWithLanguages = (0, import_codemirror_lang_styx.styx)({ nestedLanguages });
  function updateDiagnosticsDisplay(diagnostics) {
    if (diagnosticsContainer) {
      if (diagnostics.length > 0) {
        diagnosticsContainer.classList.remove("success");
        diagnosticsContainer.innerHTML = diagnostics.map((d) => {
          const loc = `${d.start}-${d.end}`;
          return `<div class="diagnostic-item"><span class="diagnostic-location">[${loc}]</span>${escapeHtml(d.message)}</div>`;
        }).join("");
      } else {
        diagnosticsContainer.classList.remove("success");
        diagnosticsContainer.innerHTML = "";
      }
    }
    onDiagnosticsChange?.(diagnostics);
  }
  function updateJsonFromStyx(source) {
    const parseResult = wasm.parse(source);
    const jsonResult = wasm.to_json(source);
    if (parseResult.diagnostics && parseResult.diagnostics.length > 0) {
      updateDiagnosticsDisplay(parseResult.diagnostics);
    } else {
      updateDiagnosticsDisplay([]);
    }
    if (jsonResult.success && jsonResult.jsonString !== void 0) {
      updatingFromStyx = true;
      jsonEditor.dispatch({
        changes: {
          from: 0,
          to: jsonEditor.state.doc.length,
          insert: jsonResult.jsonString
        }
      });
      updatingFromStyx = false;
    } else {
      updatingFromStyx = true;
      jsonEditor.dispatch({
        changes: {
          from: 0,
          to: jsonEditor.state.doc.length,
          insert: `// Error: ${jsonResult.error || "Unknown error"}`
        }
      });
      updatingFromStyx = false;
    }
  }
  function updateStyxFromJson(jsonSource) {
    const result = wasm.from_json(jsonSource);
    if (result.success && result.styxString !== void 0) {
      updatingFromJson = true;
      styxEditor.dispatch({
        changes: {
          from: 0,
          to: styxEditor.state.doc.length,
          insert: result.styxString
        }
      });
      updatingFromJson = false;
      updateDiagnosticsDisplay([]);
    }
  }
  const jsonUpdateListener = import_codemirror.EditorView.updateListener.of((update) => {
    if (update.docChanged && !updatingFromStyx) {
      if (jsonDebounceTimer) clearTimeout(jsonDebounceTimer);
      jsonDebounceTimer = setTimeout(() => {
        updateStyxFromJson(update.state.doc.toString());
      }, jsonDebounceMs);
    }
  });
  const styxUpdateListener = import_codemirror.EditorView.updateListener.of((update) => {
    if (update.docChanged && !updatingFromJson) {
      if (styxDebounceTimer) clearTimeout(styxDebounceTimer);
      styxDebounceTimer = setTimeout(() => {
        updateJsonFromStyx(update.state.doc.toString());
      }, styxDebounceMs);
    }
  });
  const jsonEditor = new import_codemirror.EditorView({
    state: import_state.EditorState.create({
      doc: "",
      extensions: [import_codemirror.basicSetup, import_theme_one_dark.oneDark, (0, import_lang_json.json)(), jsonUpdateListener, ...jsonExtensions]
    }),
    parent: jsonContainer
  });
  const styxEditor = new import_codemirror.EditorView({
    state: import_state.EditorState.create({
      doc: initialSource,
      extensions: [
        import_codemirror.basicSetup,
        import_theme_one_dark.oneDark,
        styxWithLanguages,
        styxUpdateListener,
        vimCompartment.of(vimWithPrecedence(vimEnabled)),
        ...styxExtensions
      ]
    }),
    parent: styxContainer
  });
  function enableVim() {
    styxEditor.dispatch({
      effects: vimCompartment.reconfigure(vimWithPrecedence(true))
    });
    _vimEnabled = true;
  }
  function disableVim() {
    styxEditor.dispatch({
      effects: vimCompartment.reconfigure(vimWithPrecedence(false))
    });
    _vimEnabled = false;
  }
  updateJsonFromStyx(initialSource);
  return {
    styxEditor,
    jsonEditor,
    enableVim,
    disableVim,
    isVimEnabled: () => _vimEnabled,
    getStyxSource: () => styxEditor.state.doc.toString(),
    setStyxSource: (source) => {
      styxEditor.dispatch({
        changes: {
          from: 0,
          to: styxEditor.state.doc.length,
          insert: source
        }
      });
    },
    getJsonOutput: () => jsonEditor.state.doc.toString(),
    dispose: () => {
      if (styxDebounceTimer) clearTimeout(styxDebounceTimer);
      if (jsonDebounceTimer) clearTimeout(jsonDebounceTimer);
      styxEditor.destroy();
      jsonEditor.destroy();
    }
  };
}
function escapeHtml(text) {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}
// Annotate the CommonJS export names for ESM import in node:
0 && (module.exports = {
  Compartment,
  DEFAULT_STYX_SOURCE,
  EditorState,
  EditorView,
  Prec,
  basicSetup,
  createPlayground,
  json,
  keymap,
  oneDark,
  sql,
  styx,
  vim
});
