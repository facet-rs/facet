import { describe, expect, it } from "vitest";

import type { Schema, SchemaKind, SchemaRegistry, TypeRef } from "./schema.ts";
import { buildPlan, schemaSetFromSchemas } from "./plan.ts";
import { decodeWithKind, decodeWithPlan, encodeWithKind, skipValue } from "./wire_codec.ts";

const U8_ID = 1n;
const STRING_ID = 2n;
const LIST_U8_ID = 3n;
const OPTION_STRING_ID = 4n;
const PAYLOAD_ID = 5n;
const CHANNEL_ID = 6n;
const FRAME_ID = 7n;
const REMOTE_RECORD_ID = 8n;
const LOCAL_RECORD_ID = 9n;
const BYTES_ID = 10n;
const NEVER_ID = 11n;

const u8Ref: TypeRef = { tag: "concrete", type_id: U8_ID, args: [] };
const stringRef: TypeRef = { tag: "concrete", type_id: STRING_ID, args: [] };
const listU8Ref: TypeRef = { tag: "concrete", type_id: LIST_U8_ID, args: [] };
const optionStringRef: TypeRef = { tag: "concrete", type_id: OPTION_STRING_ID, args: [] };
const payloadRef: TypeRef = { tag: "concrete", type_id: PAYLOAD_ID, args: [] };
const channelRef: TypeRef = { tag: "concrete", type_id: CHANNEL_ID, args: [] };

const registry: SchemaRegistry = new Map<bigint, Schema>([
  [U8_ID, { id: U8_ID, type_params: [], kind: { tag: "primitive", primitive_type: "u8" } }],
  [STRING_ID, { id: STRING_ID, type_params: [], kind: { tag: "primitive", primitive_type: "string" } }],
  [LIST_U8_ID, { id: LIST_U8_ID, type_params: [], kind: { tag: "list", element: u8Ref } }],
  [OPTION_STRING_ID, { id: OPTION_STRING_ID, type_params: [], kind: { tag: "option", element: stringRef } }],
  [PAYLOAD_ID, { id: PAYLOAD_ID, type_params: [], kind: { tag: "primitive", primitive_type: "payload" } }],
  [
    CHANNEL_ID,
    {
      id: CHANNEL_ID,
      type_params: [],
      kind: { tag: "channel", direction: "tx", element: u8Ref },
    },
  ],
]);

const listU8Kind = registry.get(LIST_U8_ID)!.kind;

const frameKind: SchemaKind = {
  tag: "struct",
  name: "Frame",
  fields: [
    { name: "payload", type_ref: payloadRef, required: true },
    { name: "channel", type_ref: channelRef, required: true },
  ],
};

const remoteSchemas: Schema[] = [
  ...registry.values(),
  {
    id: REMOTE_RECORD_ID,
    type_params: [],
    kind: {
      tag: "struct",
      name: "Record",
      fields: [
        { name: "payload", type_ref: listU8Ref, required: true },
        { name: "name", type_ref: stringRef, required: true },
      ],
    },
  },
];

const localSchemas: Schema[] = [
  ...registry.values(),
  {
    id: LOCAL_RECORD_ID,
    type_params: [],
    kind: {
      tag: "struct",
      name: "Record",
      fields: [
        { name: "name", type_ref: stringRef, required: true },
        { name: "payload", type_ref: listU8Ref, required: true },
        { name: "note", type_ref: optionStringRef, required: true },
      ],
    },
  },
];

describe("wire codec byte buffers", () => {
  it("roundtrips list<u8> as Uint8Array", () => {
    const input = new Uint8Array([0xde, 0xad, 0xbe, 0xef]);
    const encoded = encodeWithKind(input, listU8Kind, registry);
    const decoded = decodeWithKind(encoded, 0, listU8Kind, registry);

    expect(decoded.next).toBe(encoded.length);
    expect(decoded.value).toBeInstanceOf(Uint8Array);
    expect(Array.from(decoded.value as Uint8Array)).toEqual(Array.from(input));
    expect(skipValue(encoded, 0, listU8Kind, registry)).toBe(encoded.length);
  });

  it("roundtrips opaque payloads and channel ids", () => {
    const frame = {
      payload: new Uint8Array([1, 2, 3, 4]),
      channel: { channelId: 42n },
    };

    const encoded = encodeWithKind(frame, frameKind, registry);
    const decoded = decodeWithKind(encoded, 0, frameKind, registry);

    expect(decoded.next).toBe(encoded.length);
    expect(decoded.value).toEqual({
      payload: new Uint8Array([1, 2, 3, 4]),
      channel: 42n,
    });
    expect(skipValue(encoded, 0, frameKind, registry)).toBe(encoded.length);
  });
});

describe("wire codec translation plans", () => {
  it("treats list<u8> and bytes as the same wire shape", () => {
    const remote = schemaSetFromSchemas([
      registry.get(U8_ID)!,
      registry.get(LIST_U8_ID)!,
    ]);
    const local = schemaSetFromSchemas([
      { id: BYTES_ID, type_params: [], kind: { tag: "primitive", primitive_type: "bytes" } },
    ]);
    const plan = buildPlan(remote, local);
    const encoded = encodeWithKind(new Uint8Array([3, 1, 4]), remote.root.kind, remote.registry);
    const decoded = decodeWithPlan(
      encoded,
      0,
      plan,
      local.root.kind,
      remote.root.kind,
      new Map([...remote.registry, ...local.registry]),
    );

    expect(plan).toEqual({ tag: "identity" });
    expect(decoded.next).toBe(encoded.length);
    expect(decoded.value).toEqual(new Uint8Array([3, 1, 4]));
  });

  it("rejects primitive mismatches for never vs unit", () => {
    const remote = schemaSetFromSchemas([
      { id: NEVER_ID, type_params: [], kind: { tag: "primitive", primitive_type: "never" } },
    ]);
    const local = schemaSetFromSchemas([
      { id: 12n, type_params: [], kind: { tag: "primitive", primitive_type: "unit" } },
    ]);

    expect(() => buildPlan(remote, local)).toThrow(
      'primitive type mismatch: remote "never" vs local "unit"',
    );
  });

  it("reorders fields and preserves byte lists as Uint8Array", () => {
    const remote = schemaSetFromSchemas(remoteSchemas);
    const local = schemaSetFromSchemas(localSchemas);
    const remoteKind = remote.root.kind;
    const localKind = local.root.kind;
    const plan = buildPlan(remote, local);

    const encoded = encodeWithKind(
      {
        payload: new Uint8Array([7, 8, 9]),
        name: "alpha",
      },
      remoteKind,
      remote.registry,
    );
    const decoded = decodeWithPlan(
      encoded,
      0,
      plan,
      localKind,
      remoteKind,
      new Map([...local.registry, ...remote.registry]),
    );

    expect(decoded.next).toBe(encoded.length);
    expect(decoded.value).toEqual({
      name: "alpha",
      payload: new Uint8Array([7, 8, 9]),
      note: null,
    });
    expect((decoded.value as { payload: unknown }).payload).toBeInstanceOf(Uint8Array);
  });
});

describe("wire codec never primitive", () => {
  const neverKind: SchemaKind = { tag: "primitive", primitive_type: "never" };

  it("refuses to encode never", () => {
    expect(() => encodeWithKind(undefined, neverKind, registry)).toThrow(
      "encodePrimitive: cannot encode never",
    );
  });

  it("refuses to decode or skip never", () => {
    const bytes = new Uint8Array([0]);

    expect(() => decodeWithKind(bytes, 0, neverKind, registry)).toThrow(
      "decodePrimitive: received bytes for never primitive",
    );
    expect(() => skipValue(bytes, 0, neverKind, registry)).toThrow(
      "skipPrimitive: received bytes for never primitive",
    );
  });
});
