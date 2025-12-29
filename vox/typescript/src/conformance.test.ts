/**
 * Conformance tests for TypeScript Rapace implementation.
 *
 * These tests run the TypeScript implementation against the
 * `rapace-conformance` reference peer to validate spec compliance.
 */

import { describe, it, expect, beforeAll } from "vitest";
import { spawn } from "child_process";
import { resolve } from "path";
import {
  ConformanceRunner,
  runConformanceTest,
  ControlVerb,
  PROTOCOL_VERSION_1_0,
  Features,
  Role,
} from "./conformance-runner.js";
import { MsgDescHot, INLINE_PAYLOAD_SLOT } from "./rapace/msg-desc-hot.js";
import { FrameFlags } from "./rapace/frame-flags.js";
import { computeMethodId } from "./rapace/method-id.js";

// Path to the conformance binary (built from Rust)
const CONFORMANCE_BINARY = resolve(__dirname, "../../target/debug/rapace-conformance");

// Check if conformance binary exists
async function conformanceBinaryExists(): Promise<boolean> {
  return new Promise((resolve) => {
    const proc = spawn(CONFORMANCE_BINARY, ["--list"], { stdio: "pipe" });
    proc.on("error", () => resolve(false));
    proc.on("close", (code) => resolve(code === 0));
  });
}

describe("Conformance Tests", () => {
  let binaryExists = false;

  beforeAll(async () => {
    binaryExists = await conformanceBinaryExists();
    if (!binaryExists) {
      console.warn(
        "rapace-conformance binary not found. Run `cargo build -p rapace-conformance` first."
      );
    }
  });

  describe("Frame Format", () => {
    it.skipIf(!binaryExists)("descriptor_size", async () => {
      // This test verifies MsgDescHot is 64 bytes
      const desc = new MsgDescHot();
      const serialized = desc.serialize();
      expect(serialized.length).toBe(64);
    });

    it.skipIf(!binaryExists)("encoding_little_endian", async () => {
      // Verify little-endian encoding
      const desc = new MsgDescHot();
      desc.msgId = 0x0102030405060708n;
      desc.channelId = 0x11121314;
      desc.methodId = 0x21222324;

      const bytes = desc.serialize();

      // msgId at offset 0, little-endian
      expect(bytes[0]).toBe(0x08);
      expect(bytes[7]).toBe(0x01);

      // channelId at offset 8, little-endian
      expect(bytes[8]).toBe(0x14);
      expect(bytes[11]).toBe(0x11);

      // methodId at offset 12, little-endian
      expect(bytes[12]).toBe(0x24);
      expect(bytes[15]).toBe(0x21);
    });

    it.skipIf(!binaryExists)("sentinel_inline", async () => {
      expect(INLINE_PAYLOAD_SLOT).toBe(0xffffffff);
    });
  });

  describe("Method ID", () => {
    it("fnv1a_deterministic", () => {
      // Same input should produce same output
      const id1 = computeMethodId("Calculator", "add");
      const id2 = computeMethodId("Calculator", "add");
      expect(id1).toBe(id2);
    });

    it("fnv1a_different_methods", () => {
      // Different methods should produce different IDs
      const id1 = computeMethodId("Calculator", "add");
      const id2 = computeMethodId("Calculator", "subtract");
      expect(id1).not.toBe(id2);
    });

    it("fnv1a_non_zero", () => {
      // Method IDs should not be 0 (reserved for control)
      const id = computeMethodId("Test", "method");
      expect(id).not.toBe(0);
    });
  });

  describe("Frame Flags", () => {
    it("flag_values", () => {
      expect(FrameFlags.DATA).toBe(0b00000001);
      expect(FrameFlags.CONTROL).toBe(0b00000010);
      expect(FrameFlags.EOS).toBe(0b00000100);
      expect(FrameFlags.ERROR).toBe(0b00010000);
      expect(FrameFlags.HIGH_PRIORITY).toBe(0b00100000);
      expect(FrameFlags.CREDITS).toBe(0b01000000);
      expect(FrameFlags.NO_REPLY).toBe(0b100000000);
      expect(FrameFlags.RESPONSE).toBe(0b1000000000);
    });
  });

  // These tests require the conformance binary
  describe.skipIf(!binaryExists)("Protocol Conformance", () => {
    it("handshake.valid_hello_exchange", async () => {
      const passed = await runConformanceTest(
        "handshake.valid_hello_exchange",
        async (runner) => {
          await runner.doHandshakeAsInitiator();
        },
        CONFORMANCE_BINARY
      );

      expect(passed).toBe(true);
    });
  });
});
