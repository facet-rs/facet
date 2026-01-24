// src/index.ts
import { EditorView, basicSetup } from "codemirror";
import { EditorState, Compartment, Prec } from "@codemirror/state";
import { keymap } from "@codemirror/view";
import { oneDark } from "@codemirror/theme-one-dark";
import { json } from "@codemirror/lang-json";
import { sql } from "@codemirror/lang-sql";
import { vim } from "@replit/codemirror-vim";
import { styx } from "@bearcove/codemirror-lang-styx";
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
  return enabled ? Prec.highest(vim()) : [];
}
function createPlayground(options) {
  const {
    wasm,
    styxContainer,
    jsonContainer,
    diagnosticsContainer,
    initialSource = DEFAULT_STYX_SOURCE,
    vimEnabled = false,
    nestedLanguages = [{ tag: "sql", language: sql() }],
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
  const vimCompartment = new Compartment();
  const styxWithLanguages = styx({ nestedLanguages });
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
  const jsonUpdateListener = EditorView.updateListener.of((update) => {
    if (update.docChanged && !updatingFromStyx) {
      if (jsonDebounceTimer) clearTimeout(jsonDebounceTimer);
      jsonDebounceTimer = setTimeout(() => {
        updateStyxFromJson(update.state.doc.toString());
      }, jsonDebounceMs);
    }
  });
  const styxUpdateListener = EditorView.updateListener.of((update) => {
    if (update.docChanged && !updatingFromJson) {
      if (styxDebounceTimer) clearTimeout(styxDebounceTimer);
      styxDebounceTimer = setTimeout(() => {
        updateJsonFromStyx(update.state.doc.toString());
      }, styxDebounceMs);
    }
  });
  const jsonEditor = new EditorView({
    state: EditorState.create({
      doc: "",
      extensions: [basicSetup, oneDark, json(), jsonUpdateListener, ...jsonExtensions]
    }),
    parent: jsonContainer
  });
  const styxEditor = new EditorView({
    state: EditorState.create({
      doc: initialSource,
      extensions: [
        basicSetup,
        oneDark,
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
export {
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
};
