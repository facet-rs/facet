import { EditorView, basicSetup } from 'codemirror';
import { EditorState, Compartment, Prec, Extension } from '@codemirror/state';
import { keymap } from '@codemirror/view';
import { oneDark } from '@codemirror/theme-one-dark';
import { json } from '@codemirror/lang-json';
import { sql, SQLDialect } from '@codemirror/lang-sql';
import { vim } from '@replit/codemirror-vim';
import { styx, NestedLanguage } from '@bearcove/codemirror-lang-styx';
import type { LanguageSupport } from '@codemirror/language';

// Re-export commonly used items
export { EditorView, EditorState, Compartment, Prec, basicSetup, oneDark, json, sql, vim, styx, keymap };
export type { NestedLanguage } from '@bearcove/codemirror-lang-styx';

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
 * Options for creating a CodeMirror Styx playground.
 */
export interface PlaygroundOptions {
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
   * Initial Styx source code.
   */
  initialSource?: string;

  /**
   * Whether vim mode is enabled initially.
   * @default false
   */
  vimEnabled?: boolean;

  /**
   * Nested languages for heredoc injection.
   * @default [{ tag: 'sql', language: sql() }]
   */
  nestedLanguages?: NestedLanguage[];

  /**
   * Additional extensions for the Styx editor.
   */
  styxExtensions?: Extension[];

  /**
   * Additional extensions for the JSON editor.
   */
  jsonExtensions?: Extension[];

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
   * The Styx editor view.
   */
  styxEditor: EditorView;

  /**
   * The JSON editor view.
   */
  jsonEditor: EditorView;

  /**
   * Enable vim mode.
   */
  enableVim(): void;

  /**
   * Disable vim mode.
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
export const DEFAULT_STYX_SOURCE = `// Welcome to the Styx Playground!
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

/**
 * Create vim extension with highest precedence.
 */
function vimWithPrecedence(enabled: boolean): Extension {
  return enabled ? Prec.highest(vim()) : [];
}

/**
 * Create a CodeMirror-based Styx playground with bidirectional JSON conversion.
 *
 * @param options - Playground configuration options
 * @returns PlaygroundInstance for controlling the playground
 *
 * @example
 * ```ts
 * import init, { parse, to_json, from_json, version } from '@bearcove/styx-wasm';
 * import { createPlayground, DEFAULT_STYX_SOURCE } from '@bearcove/styx-playground-codemirror';
 *
 * const wasm = { init, parse, to_json, from_json, version };
 * await wasm.init('/path/to/styx_wasm_bg.wasm');
 *
 * const playground = createPlayground({
 *   wasm,
 *   styxContainer: document.getElementById('styx-editor'),
 *   jsonContainer: document.getElementById('json-editor'),
 *   initialSource: DEFAULT_STYX_SOURCE,
 * });
 * ```
 */
export function createPlayground(options: PlaygroundOptions): PlaygroundInstance {
  const {
    wasm,
    styxContainer,
    jsonContainer,
    diagnosticsContainer,
    initialSource = DEFAULT_STYX_SOURCE,
    vimEnabled = false,
    nestedLanguages = [{ tag: 'sql', language: sql() }],
    styxExtensions = [],
    jsonExtensions = [],
    onDiagnosticsChange,
    styxDebounceMs = 150,
    jsonDebounceMs = 300,
  } = options;

  // State
  let styxDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  let jsonDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  let updatingFromStyx = false;
  let updatingFromJson = false;
  let _vimEnabled = vimEnabled;

  // Vim compartment for dynamic toggling
  const vimCompartment = new Compartment();

  // Configure styx with nested language support
  const styxWithLanguages = styx({ nestedLanguages });

  // Update diagnostics display
  function updateDiagnosticsDisplay(diagnostics: Array<{ message: string; start: number; end: number }>) {
    if (diagnosticsContainer) {
      if (diagnostics.length > 0) {
        diagnosticsContainer.classList.remove('success');
        diagnosticsContainer.innerHTML = diagnostics
          .map((d) => {
            const loc = `${d.start}-${d.end}`;
            return `<div class="diagnostic-item"><span class="diagnostic-location">[${loc}]</span>${escapeHtml(d.message)}</div>`;
          })
          .join('');
      } else {
        diagnosticsContainer.classList.remove('success');
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
      jsonEditor.dispatch({
        changes: {
          from: 0,
          to: jsonEditor.state.doc.length,
          insert: jsonResult.jsonString,
        },
      });
      updatingFromStyx = false;
    } else {
      updatingFromStyx = true;
      jsonEditor.dispatch({
        changes: {
          from: 0,
          to: jsonEditor.state.doc.length,
          insert: `// Error: ${jsonResult.error || 'Unknown error'}`,
        },
      });
      updatingFromStyx = false;
    }
  }

  // Update Styx from JSON
  function updateStyxFromJson(jsonSource: string) {
    const result = wasm.from_json(jsonSource);
    if (result.success && result.styxString !== undefined) {
      updatingFromJson = true;
      styxEditor.dispatch({
        changes: {
          from: 0,
          to: styxEditor.state.doc.length,
          insert: result.styxString,
        },
      });
      updatingFromJson = false;
      updateDiagnosticsDisplay([]);
    }
  }

  // JSON editor update listener
  const jsonUpdateListener = EditorView.updateListener.of((update) => {
    if (update.docChanged && !updatingFromStyx) {
      if (jsonDebounceTimer) clearTimeout(jsonDebounceTimer);
      jsonDebounceTimer = setTimeout(() => {
        updateStyxFromJson(update.state.doc.toString());
      }, jsonDebounceMs);
    }
  });

  // Styx editor update listener
  const styxUpdateListener = EditorView.updateListener.of((update) => {
    if (update.docChanged && !updatingFromJson) {
      if (styxDebounceTimer) clearTimeout(styxDebounceTimer);
      styxDebounceTimer = setTimeout(() => {
        updateJsonFromStyx(update.state.doc.toString());
      }, styxDebounceMs);
    }
  });

  // Create JSON editor
  const jsonEditor = new EditorView({
    state: EditorState.create({
      doc: '',
      extensions: [basicSetup, oneDark, json(), jsonUpdateListener, ...jsonExtensions],
    }),
    parent: jsonContainer,
  });

  // Create Styx editor
  const styxEditor = new EditorView({
    state: EditorState.create({
      doc: initialSource,
      extensions: [
        basicSetup,
        oneDark,
        styxWithLanguages,
        styxUpdateListener,
        vimCompartment.of(vimWithPrecedence(vimEnabled)),
        ...styxExtensions,
      ],
    }),
    parent: styxContainer,
  });

  // Vim mode functions
  function enableVim() {
    styxEditor.dispatch({
      effects: vimCompartment.reconfigure(vimWithPrecedence(true)),
    });
    _vimEnabled = true;
  }

  function disableVim() {
    styxEditor.dispatch({
      effects: vimCompartment.reconfigure(vimWithPrecedence(false)),
    });
    _vimEnabled = false;
  }

  // Initial render
  updateJsonFromStyx(initialSource);

  return {
    styxEditor,
    jsonEditor,
    enableVim,
    disableVim,
    isVimEnabled: () => _vimEnabled,
    getStyxSource: () => styxEditor.state.doc.toString(),
    setStyxSource: (source: string) => {
      styxEditor.dispatch({
        changes: {
          from: 0,
          to: styxEditor.state.doc.length,
          insert: source,
        },
      });
    },
    getJsonOutput: () => jsonEditor.state.doc.toString(),
    dispose: () => {
      if (styxDebounceTimer) clearTimeout(styxDebounceTimer);
      if (jsonDebounceTimer) clearTimeout(jsonDebounceTimer);
      styxEditor.destroy();
      jsonEditor.destroy();
    },
  };
}

function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}
