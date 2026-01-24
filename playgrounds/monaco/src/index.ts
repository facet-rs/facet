import type * as monaco from 'monaco-editor';
import { initVimMode, VimMode } from 'monaco-vim';
import { registerStyxLanguage, catppuccinMocha, mocha, StyxTokensProvider, styxLanguageConfig } from '@bearcove/monaco-lang-styx';

// Re-export language support
export { registerStyxLanguage, catppuccinMocha, mocha, StyxTokensProvider, styxLanguageConfig } from '@bearcove/monaco-lang-styx';

// Re-export vim mode
export { initVimMode, VimMode };

/**
 * WASM module interface for Styx parsing and conversion.
 * This is expected to be provided by @bearcove/styx-wasm.
 */
export interface StyxWasm {
  init(wasmUrl?: string): Promise<void>;
  parse(source: string): ParseResult;
  to_json(source: string): ToJsonResult;
  from_json(json: string): FromJsonResult;
  version(): string;
}

export interface ParseResult {
  success: boolean;
  diagnostics?: Array<{
    message: string;
    start: number;
    end: number;
  }>;
}

export interface ToJsonResult {
  success: boolean;
  jsonString?: string;
  error?: string;
}

export interface FromJsonResult {
  success: boolean;
  styxString?: string;
  error?: string;
}

/**
 * Options for creating a Monaco Styx playground.
 */
export interface PlaygroundOptions {
  /**
   * The Monaco module instance.
   */
  monaco: typeof monaco;

  /**
   * The WASM module for Styx parsing and conversion.
   */
  wasm: StyxWasm;

  /**
   * Container element for the Styx editor.
   */
  styxContainer: HTMLElement;

  /**
   * Container element for the JSON output editor.
   */
  jsonContainer: HTMLElement;

  /**
   * Optional container for diagnostics output.
   */
  diagnosticsContainer?: HTMLElement;

  /**
   * Optional container for vim status bar.
   */
  vimStatusContainer?: HTMLElement;

  /**
   * Initial Styx source code.
   */
  initialSource?: string;

  /**
   * Whether vim mode is enabled initially.
   * @default false
   */
  vimEnabled?: boolean;

  /**
   * Embedded language definitions for heredoc injection.
   * Pass monaco basic language modules like sql, javascript, etc.
   */
  embeddedLanguages?: Array<{
    id: string;
    def: {
      conf: monaco.languages.LanguageConfiguration;
      language: monaco.languages.IMonarchLanguage;
    };
  }>;

  /**
   * Callback when diagnostics change.
   */
  onDiagnosticsChange?: (diagnostics: Array<{ message: string; start: number; end: number }>) => void;

  /**
   * Debounce delay for Styx -> JSON conversion (ms).
   * @default 150
   */
  styxDebounceMs?: number;

  /**
   * Debounce delay for JSON -> Styx conversion (ms).
   * @default 300
   */
  jsonDebounceMs?: number;
}

/**
 * Playground instance returned by createPlayground.
 */
export interface PlaygroundInstance {
  /**
   * The Styx editor instance.
   */
  styxEditor: monaco.editor.IStandaloneCodeEditor;

  /**
   * The JSON editor instance.
   */
  jsonEditor: monaco.editor.IStandaloneCodeEditor;

  /**
   * Enable vim mode on both editors.
   */
  enableVim(): void;

  /**
   * Disable vim mode on both editors.
   */
  disableVim(): void;

  /**
   * Check if vim mode is currently enabled.
   */
  isVimEnabled(): boolean;

  /**
   * Get the current Styx source.
   */
  getStyxSource(): string;

  /**
   * Set the Styx source.
   */
  setStyxSource(source: string): void;

  /**
   * Get the current JSON output.
   */
  getJsonOutput(): string;

  /**
   * Dispose of the playground and clean up resources.
   */
  dispose(): void;
}

/**
 * Default Styx source for demonstration.
 */
export const DEFAULT_STYX_SOURCE = `// Welcome to the Styx Monaco Playground!
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

/**
 * Create a Monaco-based Styx playground with bidirectional JSON conversion.
 *
 * @param options - Playground configuration options
 * @returns PlaygroundInstance for controlling the playground
 *
 * @example
 * ```ts
 * import * as monaco from 'monaco-editor';
 * import init, { parse, to_json, from_json, version } from '@bearcove/styx-wasm';
 * import { createPlayground, DEFAULT_STYX_SOURCE } from '@bearcove/styx-playground-monaco';
 *
 * const wasm = { init, parse, to_json, from_json, version };
 * await wasm.init('/path/to/styx_wasm_bg.wasm');
 *
 * const playground = createPlayground({
 *   monaco,
 *   wasm,
 *   styxContainer: document.getElementById('styx-editor'),
 *   jsonContainer: document.getElementById('json-editor'),
 *   initialSource: DEFAULT_STYX_SOURCE,
 * });
 * ```
 */
export function createPlayground(options: PlaygroundOptions): PlaygroundInstance {
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
    jsonDebounceMs = 300,
  } = options;

  // Register Styx language
  registerStyxLanguage(monacoInstance, embeddedLanguages);

  // State
  let styxDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  let jsonDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  let updatingFromStyx = false;
  let updatingFromJson = false;
  let vimModeStyxEditor: VimMode | null = null;
  let vimModeJsonEditor: VimMode | null = null;
  let _vimEnabled = vimEnabled;

  // Editor options
  const editorOptions: monaco.editor.IStandaloneEditorConstructionOptions = {
    theme: 'catppuccin-mocha',
    automaticLayout: true,
    minimap: { enabled: false },
    fontSize: 14,
    lineNumbers: 'on',
    renderLineHighlight: 'all',
    scrollBeyondLastLine: false,
    wordWrap: 'on',
    fontFamily: "'SF Mono', 'Monaco', 'Menlo', 'Consolas', monospace",
  };

  // Create Styx editor
  const styxEditor = monacoInstance.editor.create(styxContainer, {
    ...editorOptions,
    value: initialSource,
    language: 'styx',
  });

  // Create JSON editor
  const jsonEditor = monacoInstance.editor.create(jsonContainer, {
    ...editorOptions,
    value: '',
    language: 'json',
  });

  // Update diagnostics display
  function updateDiagnosticsDisplay(diagnostics: Array<{ message: string; start: number; end: number }>) {
    if (diagnosticsContainer) {
      if (diagnostics.length > 0) {
        diagnosticsContainer.innerHTML = diagnostics
          .map((d) => {
            const loc = `${d.start}-${d.end}`;
            return `<div class="diagnostic-item"><span class="diagnostic-location">[${loc}]</span>${escapeHtml(d.message)}</div>`;
          })
          .join('');
      } else {
        diagnosticsContainer.innerHTML = '';
      }
    }
    onDiagnosticsChange?.(diagnostics);
  }

  // Update JSON from Styx
  function updateJsonFromStyx(source: string) {
    const parseResult = wasm.parse(source);
    const jsonResult = wasm.to_json(source);

    if (parseResult.diagnostics && parseResult.diagnostics.length > 0) {
      updateDiagnosticsDisplay(parseResult.diagnostics);
    } else {
      updateDiagnosticsDisplay([]);
    }

    if (jsonResult.success && jsonResult.jsonString !== undefined) {
      updatingFromStyx = true;
      jsonEditor.setValue(jsonResult.jsonString);
      updatingFromStyx = false;
    } else {
      updatingFromStyx = true;
      jsonEditor.setValue(`// Error: ${jsonResult.error || 'Unknown error'}`);
      updatingFromStyx = false;
    }
  }

  // Update Styx from JSON
  function updateStyxFromJson(jsonSource: string) {
    const result = wasm.from_json(jsonSource);
    if (result.success && result.styxString !== undefined) {
      updatingFromJson = true;
      styxEditor.setValue(result.styxString);
      updatingFromJson = false;
      updateDiagnosticsDisplay([]);
    }
  }

  // Styx -> JSON on change
  styxEditor.onDidChangeModelContent(() => {
    if (!updatingFromJson) {
      if (styxDebounceTimer) clearTimeout(styxDebounceTimer);
      styxDebounceTimer = setTimeout(() => {
        updateJsonFromStyx(styxEditor.getValue());
      }, styxDebounceMs);
    }
  });

  // JSON -> Styx on change
  jsonEditor.onDidChangeModelContent(() => {
    if (!updatingFromStyx) {
      if (jsonDebounceTimer) clearTimeout(jsonDebounceTimer);
      jsonDebounceTimer = setTimeout(() => {
        updateStyxFromJson(jsonEditor.getValue());
      }, jsonDebounceMs);
    }
  });

  // Vim mode functions
  function enableVim() {
    if (!vimModeStyxEditor && vimStatusContainer) {
      vimModeStyxEditor = initVimMode(styxEditor, vimStatusContainer);
    }
    if (!vimModeJsonEditor && vimStatusContainer) {
      vimModeJsonEditor = initVimMode(jsonEditor, vimStatusContainer);
    }
    if (vimStatusContainer) {
      vimStatusContainer.style.display = 'block';
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
      vimStatusContainer.textContent = '';
      vimStatusContainer.style.display = 'none';
    }
    _vimEnabled = false;
  }

  // Initial vim setup
  if (vimEnabled) {
    enableVim();
  } else if (vimStatusContainer) {
    vimStatusContainer.style.display = 'none';
  }

  // Initial render
  updateJsonFromStyx(initialSource);

  return {
    styxEditor,
    jsonEditor,
    enableVim,
    disableVim,
    isVimEnabled: () => _vimEnabled,
    getStyxSource: () => styxEditor.getValue(),
    setStyxSource: (source: string) => styxEditor.setValue(source),
    getJsonOutput: () => jsonEditor.getValue(),
    dispose: () => {
      disableVim();
      if (styxDebounceTimer) clearTimeout(styxDebounceTimer);
      if (jsonDebounceTimer) clearTimeout(jsonDebounceTimer);
      styxEditor.dispose();
      jsonEditor.dispose();
    },
  };
}

function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

/**
 * Setup Monaco web workers for JSON validation and other languages.
 * Call this before creating editors.
 *
 * @example
 * ```ts
 * import { setupMonacoWorkers } from '@bearcove/styx-playground-monaco';
 *
 * setupMonacoWorkers();
 * ```
 */
export function setupMonacoWorkers(): void {
  // @ts-ignore
  self.MonacoEnvironment = {
    getWorker(_: unknown, label: string) {
      if (label === 'json') {
        return new Worker(
          new URL('monaco-editor/esm/vs/language/json/json.worker.js', import.meta.url),
          { type: 'module' }
        );
      }
      if (label === 'css' || label === 'scss' || label === 'less') {
        return new Worker(
          new URL('monaco-editor/esm/vs/language/css/css.worker.js', import.meta.url),
          { type: 'module' }
        );
      }
      if (label === 'html' || label === 'handlebars' || label === 'razor') {
        return new Worker(
          new URL('monaco-editor/esm/vs/language/html/html.worker.js', import.meta.url),
          { type: 'module' }
        );
      }
      if (label === 'typescript' || label === 'javascript') {
        return new Worker(
          new URL('monaco-editor/esm/vs/language/typescript/ts.worker.js', import.meta.url),
          { type: 'module' }
        );
      }
      return new Worker(
        new URL('monaco-editor/esm/vs/editor/editor.worker.js', import.meta.url),
        { type: 'module' }
      );
    },
  };
}
