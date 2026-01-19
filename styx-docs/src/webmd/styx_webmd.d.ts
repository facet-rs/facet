/* tslint:disable */
/* eslint-disable */

/**
 * Render markdown to HTML with syntax highlighting for styx code blocks
 */
export function render_markdown(input: string): Promise<string>;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly render_markdown: (a: number, b: number) => any;
    readonly abort: () => void;
    readonly calloc: (a: number, b: number) => number;
    readonly clock: () => number;
    readonly dup: (a: number) => number;
    readonly fclose: (a: number) => number;
    readonly fdopen: (a: number, b: number) => number;
    readonly fputc: (a: number, b: number) => number;
    readonly fputs: (a: number, b: number) => number;
    readonly free: (a: number) => void;
    readonly fwrite: (a: number, b: number, c: number, d: number) => number;
    readonly iswalnum: (a: number) => number;
    readonly iswalpha: (a: number) => number;
    readonly iswdigit: (a: number) => number;
    readonly iswlower: (a: number) => number;
    readonly iswspace: (a: number) => number;
    readonly iswupper: (a: number) => number;
    readonly iswxdigit: (a: number) => number;
    readonly malloc: (a: number) => number;
    readonly memchr: (a: number, b: number, c: number) => number;
    readonly realloc: (a: number, b: number) => number;
    readonly strchr: (a: number, b: number) => number;
    readonly strcmp: (a: number, b: number) => number;
    readonly strncmp: (a: number, b: number, c: number) => number;
    readonly strncpy: (a: number, b: number, c: number) => number;
    readonly towlower: (a: number) => number;
    readonly towupper: (a: number) => number;
    readonly wasm_bindgen__closure__destroy__h2f7fb7d0886ed8c7: (a: number, b: number) => void;
    readonly wasm_bindgen__convert__closures_____invoke__hc8a8310e48c5c33a: (a: number, b: number, c: any, d: any) => void;
    readonly wasm_bindgen__convert__closures_____invoke__hf5df86b8ecc6dc2c: (a: number, b: number, c: any) => void;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
