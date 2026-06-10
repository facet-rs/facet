import { readdirSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { decodeTyped } from "@bearcove/phon-engine";
import { registry, schemaId } from "./wire.phon.generated.ts";

const WIRE = new URL("../../../../test-fixtures/golden-vectors/wire/", import.meta.url);

describe("phon Message decode (all golden wire vectors)", () => {
  const files = readdirSync(WIRE).filter((f) => f.endsWith(".bin"));
  for (const name of files) {
    it(`decodes ${name}`, () => {
      const bytes = new Uint8Array(readFileSync(new URL(name, WIRE)));
      const msg = decodeTyped(bytes, schemaId.Message, schemaId.Message, registry) as {
        connection_id: bigint;
        payload: { tag: string };
      };
      expect(typeof msg.connection_id).toBe("bigint");
      expect(typeof msg.payload.tag).toBe("string");
    });
  }
});
