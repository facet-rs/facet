/**
 * Conformance tests for TypeScript Rapace implementation.
 *
 * These tests run the TypeScript implementation against the
 * `rapace-conformance` reference peer to validate spec compliance.
 */

import { describe, it, expect, beforeAll } from "vitest";
import { spawn, execSync } from "child_process";
import { resolve } from "path";
import { existsSync } from "fs";
import {
  ConformanceRunner,
  runConformanceTest,
} from "./conformance-runner.js";
import { MsgDescHot, INLINE_PAYLOAD_SLOT } from "./rapace/msg-desc-hot.js";
import { FrameFlags } from "./rapace/frame-flags.js";
import { computeMethodId } from "./rapace/method-id.js";

interface ConformanceTestCase {
  name: string;
  rules: string[];
}

// Path to the conformance binary (built from Rust)
function getConformanceBinary(): string {
  const debugPath = resolve(__dirname, "../../target/debug/rapace-conformance");
  const releasePath = resolve(__dirname, "../../target/release/rapace-conformance");
  
  if (existsSync(debugPath)) return debugPath;
  if (existsSync(releasePath)) return releasePath;
  return "rapace-conformance"; // Fall back to PATH
}

const CONFORMANCE_BINARY = getConformanceBinary();

// Get all test cases from the conformance harness
function getTestCases(): ConformanceTestCase[] {
  try {
    const output = execSync(`"${CONFORMANCE_BINARY}" --list --format json`, {
      encoding: "utf-8",
      stdio: ["pipe", "pipe", "pipe"],
    });
    return JSON.parse(output);
  } catch (e) {
    console.warn("Failed to get test cases from conformance harness:", e);
    return [];
  }
}

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
  let testCases: ConformanceTestCase[] = [];

  beforeAll(async () => {
    binaryExists = await conformanceBinaryExists();
    if (!binaryExists) {
      console.warn(
        "rapace-conformance binary not found. Run `cargo build -p rapace-conformance` first."
      );
    } else {
      testCases = getTestCases();
      console.log(`Found ${testCases.length} conformance test cases`);
    }
  });

  // Run all conformance tests from the harness
  describe("All Conformance Tests", () => {
    it("should run all test cases from harness", async () => {
      if (!binaryExists) {
        console.warn("Skipping: conformance binary not found");
        return;
      }

      if (testCases.length === 0) {
        throw new Error("No test cases found from conformance harness");
      }

      const failures: Array<{ name: string; error: string }> = [];

      for (const testCase of testCases) {
        try {
          const passed = await runConformanceTest(
            testCase.name,
            async (runner) => {
              // For interactive tests, try to do handshake
              if (
                testCase.name.startsWith("handshake.") ||
                testCase.name.startsWith("call.") ||
                testCase.name.startsWith("channel.") ||
                testCase.name.startsWith("control.") ||
                testCase.name.startsWith("cancel.")
              ) {
                try {
                  await runner.doHandshakeAsInitiator();
                } catch {
                  // Some tests may not need handshake
                }
              }
            },
            CONFORMANCE_BINARY
          );

          if (!passed) {
            failures.push({ name: testCase.name, error: "test failed" });
          }
        } catch (e) {
          failures.push({ name: testCase.name, error: String(e) });
        }
      }

      if (failures.length > 0) {
        const failureMessages = failures
          .map((f) => `  - ${f.name}: ${f.error}`)
          .join("\n");
        throw new Error(
          `${failures.length}/${testCases.length} conformance tests failed:\n${failureMessages}`
        );
      }
    });
  });

  // Structural tests that don't need the harness
  describe("Frame Format", () => {
    it("descriptor_size", () => {
      const desc = new MsgDescHot();
      const serialized = desc.serialize();
      expect(serialized.length).toBe(64);
    });

    it("encoding_little_endian", () => {
      const desc = new MsgDescHot();
      desc.msgId = 0x0102030405060708n;
      desc.channelId = 0x11121314;
      desc.methodId = 0x21222324;

      const bytes = desc.serialize();

      expect(bytes[0]).toBe(0x08);
      expect(bytes[7]).toBe(0x01);
      expect(bytes[8]).toBe(0x14);
      expect(bytes[11]).toBe(0x11);
      expect(bytes[12]).toBe(0x24);
      expect(bytes[15]).toBe(0x21);
    });

    it("sentinel_inline", () => {
      expect(INLINE_PAYLOAD_SLOT).toBe(0xffffffff);
    });
  });

  describe("Method ID", () => {
    it("fnv1a_deterministic", () => {
      const id1 = computeMethodId("Calculator", "add");
      const id2 = computeMethodId("Calculator", "add");
      expect(id1).toBe(id2);
    });

    it("fnv1a_different_methods", () => {
      const id1 = computeMethodId("Calculator", "add");
      const id2 = computeMethodId("Calculator", "subtract");
      expect(id1).not.toBe(id2);
    });

    it("fnv1a_non_zero", () => {
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
});
