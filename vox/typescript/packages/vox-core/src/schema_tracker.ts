// Schema exchange on the phon engine.
//
// A peer advertises its type for a (method, direction) binding as a phon
// schema-closure (self-describing bytes) in the `schemas:` field. The receiver
// records the writer closure and builds a compatibility decoder
// against the local reader type. Field matching, reordering, and defaulting are
// phon's compatibility plan; vox only records the
// writer closure and asks phon to build the decoder.
//
// r[impl schema.principles.self-describing]
// r[impl schema.tracking.received]

import { type Registry, type Schema, hexToBytes } from "@bearcove/phon-schema";
import { type Typed, decodeTyped } from "@bearcove/phon-engine";
import { parseSchemaClosure } from "@bearcove/vox-wire";

/** A reusable compat decoder yielding the ergonomic `{ tag, value }` shape. */
export type TypedDecoder = (bytes: Uint8Array) => Typed;

export type BindingDirection = "args" | "response";

/** Per-method schema data emitted by vox-codegen (`{service}Methods`). */
export interface PhonChannelMeta {
  index: number;
  direction: "tx" | "rx";
  elementRoot: bigint;
}
export interface PhonMethodSchemas {
  argsRoot: bigint;
  argsSchemaClosure: string;
  okRoot: bigint;
  /** Root of the response wire type `Result<T, VoxError<E>>` (server encode). */
  responseRoot: bigint;
  /** Schema-closure hex for the response wire type (advertised by the server). */
  responseSchemaClosure: string;
  channels: PhonChannelMeta[];
}

const bindingKey = (methodId: bigint, direction: BindingDirection): string =>
  `${methodId}:${direction}`;

const decoderKeyPrefix = (methodId: bigint, direction: BindingDirection): string =>
  `${bindingKey(methodId, direction)}:`;

interface ReceivedBinding {
  root: bigint;
  schemas: Schema[];
  auxiliaryRoots: Map<string, bigint>;
}

/**
 * Tracks the writer schema closures a peer advertised, and builds compat decoders
 * against local reader roots.
 */
// r[impl schema.tracking.received]
// r[impl schema.type-id.per-connection]
export class SchemaTracker {
  private received = new Map<string, ReceivedBinding>();
  // Cache of built decoders, keyed by (method, direction, readerRoot).
  private decoders = new Map<string, TypedDecoder>();

  reset(): void {
    this.received.clear();
    this.decoders.clear();
  }

  /**
   * Record the peer's phon schema-closure bytes for a binding. Best-effort and
   * idempotent — receiving a schema again simply overwrites (best-effort).
   */
  // r[impl schema.tracking.bindings]
  recordReceived(methodId: bigint, direction: BindingDirection, schemaBytes: Uint8Array): void {
    if (schemaBytes.length === 0) return;
    const parsed = parseSchemaClosure(schemaBytes);
    this.received.set(bindingKey(methodId, direction), {
      root: parsed.root,
      schemas: parsed.schemas,
      auxiliaryRoots: new Map(parsed.auxiliaryRoots.map((root) => [root.role, root.root])),
    });
    const prefix = decoderKeyPrefix(methodId, direction);
    for (const key of this.decoders.keys()) {
      if (key.startsWith(prefix)) {
        this.decoders.delete(key);
      }
    }
  }

  hasReceived(methodId: bigint, direction: BindingDirection): boolean {
    return this.received.has(bindingKey(methodId, direction));
  }

  // r[impl schema.exchange.required]
  requireReceived(methodId: bigint, direction: BindingDirection): void {
    if (this.hasReceived(methodId, direction)) return;
    throw new SchemaCompatibilityError(
      `missing ${direction} schema binding for method ${methodId}; sender must send schemas before data`,
    );
  }

  /**
   * Build (and cache) a compat decoder for `(methodId, direction)` producing the
   * reader type identified by `readerRoot`, resolved through `local` plus the
   * writer's exchanged schemas. Returns null when no writer schema was received.
   */
  // r[impl schema.errors.call-level]
  buildDecoder(
    methodId: bigint,
    direction: BindingDirection,
    readerRoot: bigint,
    local: Registry,
  ): TypedDecoder | null {
    const writer = this.received.get(bindingKey(methodId, direction));
    if (!writer) return null;
    const cacheKey = `${bindingKey(methodId, direction)}:${readerRoot}`;
    const cached = this.decoders.get(cacheKey);
    if (cached) return cached;
    const reg = local.with(writer.schemas);
    const decoder: TypedDecoder = (bytes) => decodeTyped(bytes, writer.root, readerRoot, reg);
    this.decoders.set(cacheKey, decoder);
    return decoder;
  }

  auxiliaryRoot(
    methodId: bigint,
    direction: BindingDirection,
    role: string,
  ): bigint | null {
    return this.received.get(bindingKey(methodId, direction))?.auxiliaryRoots.get(role) ?? null;
  }

  // r[impl schema.exchange.channels]
  buildAuxiliaryDecoder(
    methodId: bigint,
    direction: BindingDirection,
    role: string,
    readerRoot: bigint,
    local: Registry,
  ): TypedDecoder | null {
    const writer = this.received.get(bindingKey(methodId, direction));
    if (!writer) return null;
    const writerRoot = writer.auxiliaryRoots.get(role);
    if (writerRoot === undefined) return null;
    const cacheKey = `${bindingKey(methodId, direction)}:${role}:${readerRoot}`;
    const cached = this.decoders.get(cacheKey);
    if (cached) return cached;
    const reg = local.with(writer.schemas);
    const decoder: TypedDecoder = (bytes) => decodeTyped(bytes, writerRoot, readerRoot, reg);
    this.decoders.set(cacheKey, decoder);
    return decoder;
  }

  /**
   * Decode against the writer's OWN advertised schema (writer == reader). Used for
   * responses, whose wire type is `Result<T, VoxError<E>>` — the server advertises
   * it and we decode the `{ tag: "Ok" | "Err", value }` structure directly. `local`
   * supplies the primitive table; the writer closure supplies every composite.
   */
  buildWriterDecoder(
    methodId: bigint,
    direction: BindingDirection,
    local: Registry,
  ): TypedDecoder | null {
    const writer = this.received.get(bindingKey(methodId, direction));
    if (!writer) return null;
    const cacheKey = `${bindingKey(methodId, direction)}:writer`;
    const cached = this.decoders.get(cacheKey);
    if (cached) return cached;
    const reg = local.with(writer.schemas);
    const decoder: TypedDecoder = (bytes) => decodeTyped(bytes, writer.root, writer.root, reg);
    this.decoders.set(cacheKey, decoder);
    return decoder;
  }
}

export class SchemaCompatibilityError extends Error {
  constructor(message: string) {
    super(`Schema compatibility error: ${message}`);
    this.name = "SchemaCompatibilityError";
  }
}

/**
 * Tracks which (method, direction) schema closures have been advertised on a
 * connection, so each is sent at most once.
 */
// r[impl schema.tracking.sent]
// r[impl schema.tracking.bindings]
export class SchemaSendTracker {
  private sent = new Set<string>();

  reset(): void {
    this.sent.clear();
  }

  /**
   * The phon schema-closure bytes (as a `number[]` for the `schemas:` wire field)
   * to advertise for `(methodId, direction)`, or `[]` when already sent. The
   * closure hex comes from the generated `{service}Methods` table.
   */
  // r[impl schema.format.delivery]
  // r[impl schema.exchange.idempotent]
  // r[impl schema.principles.sender-driven]
  // r[impl schema.principles.no-roundtrips]
  prepareSchemas(methodId: bigint, direction: BindingDirection, closureHex: string): number[] {
    const key = bindingKey(methodId, direction);
    if (this.sent.has(key)) return [];
    this.sent.add(key);
    return Array.from(hexToBytes(closureHex));
  }
}
