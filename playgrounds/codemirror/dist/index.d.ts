import { EditorView } from 'codemirror';
export { EditorView, basicSetup } from 'codemirror';
import { Extension } from '@codemirror/state';
export { Compartment, EditorState, Prec } from '@codemirror/state';
export { keymap } from '@codemirror/view';
export { oneDark } from '@codemirror/theme-one-dark';
export { json } from '@codemirror/lang-json';
export { sql } from '@codemirror/lang-sql';
export { vim } from '@replit/codemirror-vim';
import { NestedLanguage } from '@bearcove/codemirror-lang-styx';
export { NestedLanguage, styx } from '@bearcove/codemirror-lang-styx';

/**
 * WASM module interface for Styx parsing and conversion.
 * This is expected to be provided by @bearcove/styx-wasm.
 */
interface StyxWasm {
    init(wasmUrl?: string): Promise<void>;
    parse(source: string): ParseResult;
    to_json(source: string): ToJsonResult;
    from_json(json: string): FromJsonResult;
    version(): string;
}
interface ParseResult {
    success: boolean;
    diagnostics?: Array<{
        message: string;
        start: number;
        end: number;
    }>;
}
interface ToJsonResult {
    success: boolean;
    jsonString?: string;
    error?: string;
}
interface FromJsonResult {
    success: boolean;
    styxString?: string;
    error?: string;
}
/**
 * Options for creating a CodeMirror Styx playground.
 */
interface PlaygroundOptions {
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
    onDiagnosticsChange?: (diagnostics: Array<{
        message: string;
        start: number;
        end: number;
    }>) => void;
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
interface PlaygroundInstance {
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
declare const DEFAULT_STYX_SOURCE = "// Welcome to the Styx Playground!\n// Edit this document and see the JSON output on the right.\n\n/// Server configuration\nserver {\n    host localhost\n    port 8080\n\n    // Tags add semantic meaning to values\n    timeout @duration(30s)\n\n    tls {\n        enabled true\n        cert /etc/ssl/cert.pem\n        key /etc/ssl/key.pem\n    }\n}\n\n/// Database settings\ndatabase {\n    url postgres://localhost/myapp\n    pool_size 10\n    max_connections @int(100)\n}\n\n/// Feature flags (a sequence)\nfeatures (\n    dark-mode\n    notifications\n    @experimental(analytics)\n)\n\n/// Multi-line content with heredocs\nquery <<SQL,sql\nSELECT id, name, email\nFROM users\nWHERE active = true\nSQL\n";
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
declare function createPlayground(options: PlaygroundOptions): PlaygroundInstance;

export { DEFAULT_STYX_SOURCE, type FromJsonResult, type ParseResult, type PlaygroundInstance, type PlaygroundOptions, type StyxWasm, type ToJsonResult, createPlayground };
