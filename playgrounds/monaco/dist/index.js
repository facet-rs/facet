// src/index.ts
import { initVimMode, VimMode } from "monaco-vim";
import { registerStyxLanguage } from "@bearcove/monaco-lang-styx";
import { registerStyxLanguage as registerStyxLanguage2, catppuccinMocha as catppuccinMocha2, mocha as mocha2, StyxTokensProvider as StyxTokensProvider2, styxLanguageConfig as styxLanguageConfig2 } from "@bearcove/monaco-lang-styx";
var DEFAULT_STYX_SOURCE = `// Welcome to the Styx Monaco Playground!
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

/// Multi-line content with heredocs (language hint after comma)
query <<SQL,sql
SELECT id, name, email
FROM users
WHERE active = true
SQL
`;
function createPlayground(options) {
  const {
    monaco: monacoInstance,
    wasm,
    styxContainer,
    jsonContainer,
    diagnosticsContainer,
    vimStatusContainer,
    initialSource = DEFAULT_STYX_SOURCE,
    vimEnabled = false,
    embeddedLanguages,
    onDiagnosticsChange,
    styxDebounceMs = 150,
    jsonDebounceMs = 300
  } = options;
  registerStyxLanguage(monacoInstance, embeddedLanguages);
  let styxDebounceTimer = null;
  let jsonDebounceTimer = null;
  let updatingFromStyx = false;
  let updatingFromJson = false;
  let vimModeStyxEditor = null;
  let vimModeJsonEditor = null;
  let _vimEnabled = vimEnabled;
  const editorOptions = {
    theme: "catppuccin-mocha",
    automaticLayout: true,
    minimap: { enabled: false },
    fontSize: 14,
    lineNumbers: "on",
    renderLineHighlight: "all",
    scrollBeyondLastLine: false,
    wordWrap: "on",
    fontFamily: "'SF Mono', 'Monaco', 'Menlo', 'Consolas', monospace"
  };
  const styxEditor = monacoInstance.editor.create(styxContainer, {
    ...editorOptions,
    value: initialSource,
    language: "styx"
  });
  const jsonEditor = monacoInstance.editor.create(jsonContainer, {
    ...editorOptions,
    value: "",
    language: "json"
  });
  function updateDiagnosticsDisplay(diagnostics) {
    if (diagnosticsContainer) {
      if (diagnostics.length > 0) {
        diagnosticsContainer.innerHTML = diagnostics.map((d) => {
          const loc = `${d.start}-${d.end}`;
          return `<div class="diagnostic-item"><span class="diagnostic-location">[${loc}]</span>${escapeHtml(d.message)}</div>`;
        }).join("");
      } else {
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
      jsonEditor.setValue(jsonResult.jsonString);
      updatingFromStyx = false;
    } else {
      updatingFromStyx = true;
      jsonEditor.setValue(`// Error: ${jsonResult.error || "Unknown error"}`);
      updatingFromStyx = false;
    }
  }
  function updateStyxFromJson(jsonSource) {
    const result = wasm.from_json(jsonSource);
    if (result.success && result.styxString !== void 0) {
      updatingFromJson = true;
      styxEditor.setValue(result.styxString);
      updatingFromJson = false;
      updateDiagnosticsDisplay([]);
    }
  }
  styxEditor.onDidChangeModelContent(() => {
    if (!updatingFromJson) {
      if (styxDebounceTimer) clearTimeout(styxDebounceTimer);
      styxDebounceTimer = setTimeout(() => {
        updateJsonFromStyx(styxEditor.getValue());
      }, styxDebounceMs);
    }
  });
  jsonEditor.onDidChangeModelContent(() => {
    if (!updatingFromStyx) {
      if (jsonDebounceTimer) clearTimeout(jsonDebounceTimer);
      jsonDebounceTimer = setTimeout(() => {
        updateStyxFromJson(jsonEditor.getValue());
      }, jsonDebounceMs);
    }
  });
  function enableVim() {
    if (!vimModeStyxEditor && vimStatusContainer) {
      vimModeStyxEditor = initVimMode(styxEditor, vimStatusContainer);
    }
    if (!vimModeJsonEditor && vimStatusContainer) {
      vimModeJsonEditor = initVimMode(jsonEditor, vimStatusContainer);
    }
    if (vimStatusContainer) {
      vimStatusContainer.style.display = "block";
    }
    _vimEnabled = true;
  }
  function disableVim() {
    if (vimModeStyxEditor) {
      vimModeStyxEditor.dispose();
      vimModeStyxEditor = null;
    }
    if (vimModeJsonEditor) {
      vimModeJsonEditor.dispose();
      vimModeJsonEditor = null;
    }
    if (vimStatusContainer) {
      vimStatusContainer.textContent = "";
      vimStatusContainer.style.display = "none";
    }
    _vimEnabled = false;
  }
  if (vimEnabled) {
    enableVim();
  } else if (vimStatusContainer) {
    vimStatusContainer.style.display = "none";
  }
  updateJsonFromStyx(initialSource);
  return {
    styxEditor,
    jsonEditor,
    enableVim,
    disableVim,
    isVimEnabled: () => _vimEnabled,
    getStyxSource: () => styxEditor.getValue(),
    setStyxSource: (source) => styxEditor.setValue(source),
    getJsonOutput: () => jsonEditor.getValue(),
    dispose: () => {
      disableVim();
      if (styxDebounceTimer) clearTimeout(styxDebounceTimer);
      if (jsonDebounceTimer) clearTimeout(jsonDebounceTimer);
      styxEditor.dispose();
      jsonEditor.dispose();
    }
  };
}
function escapeHtml(text) {
  const div = document.createElement("div");
  div.textContent = text;
  return div.innerHTML;
}
function setupMonacoWorkers() {
  self.MonacoEnvironment = {
    getWorker(_, label) {
      if (label === "json") {
        return new Worker(
          new URL("monaco-editor/esm/vs/language/json/json.worker.js", import.meta.url),
          { type: "module" }
        );
      }
      if (label === "css" || label === "scss" || label === "less") {
        return new Worker(
          new URL("monaco-editor/esm/vs/language/css/css.worker.js", import.meta.url),
          { type: "module" }
        );
      }
      if (label === "html" || label === "handlebars" || label === "razor") {
        return new Worker(
          new URL("monaco-editor/esm/vs/language/html/html.worker.js", import.meta.url),
          { type: "module" }
        );
      }
      if (label === "typescript" || label === "javascript") {
        return new Worker(
          new URL("monaco-editor/esm/vs/language/typescript/ts.worker.js", import.meta.url),
          { type: "module" }
        );
      }
      return new Worker(
        new URL("monaco-editor/esm/vs/editor/editor.worker.js", import.meta.url),
        { type: "module" }
      );
    }
  };
}
export {
  DEFAULT_STYX_SOURCE,
  StyxTokensProvider2 as StyxTokensProvider,
  VimMode,
  catppuccinMocha2 as catppuccinMocha,
  createPlayground,
  initVimMode,
  mocha2 as mocha,
  registerStyxLanguage2 as registerStyxLanguage,
  setupMonacoWorkers,
  styxLanguageConfig2 as styxLanguageConfig
};
