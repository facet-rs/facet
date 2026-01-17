/* tslint:disable */
/* eslint-disable */

/**
 * Convert a JSON string to Styx format.
 *
 * Returns a Styx document string representation of the JSON.
 * Tagged values ({"$tag": "name", "$value": ...}) are converted back to tags.
 */
export function from_json(json_source: string): any;

/**
 * Parse a Styx document and return diagnostics.
 *
 * Returns a JSON object with `success` boolean and `diagnostics` array.
 */
export function parse(source: string): any;

/**
 * Convert a Styx document to JSON.
 *
 * Returns a JSON string representation of the Styx document.
 * Tags are represented as `{"$tag": "tagname", "$value": ...}`.
 * Returns an error object if parsing fails.
 */
export function to_json(source: string): any;

/**
 * Validate a Styx document and return whether it's valid.
 */
export function validate(source: string): boolean;

/**
 * Get the version of the Styx WASM library.
 */
export function version(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly from_json: (a: number, b: number) => any;
    readonly parse: (a: number, b: number) => any;
    readonly to_json: (a: number, b: number) => any;
    readonly validate: (a: number, b: number) => number;
    readonly version: () => [number, number];
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
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
