/**
 * Conformance test runner for TypeScript Rapace implementation.
 *
 * This module spawns the `rapace-conformance` binary and communicates
 * with it via stdin/stdout to validate the TypeScript implementation
 * against the spec.
 */

import { spawn, ChildProcess } from "child_process";
import { MsgDescHot, MSG_DESC_HOT_SIZE, INLINE_PAYLOAD_SLOT } from "./rapace/msg-desc-hot.js";
import { FrameFlags } from "./rapace/frame-flags.js";

/** Control verb constants */
export const ControlVerb = {
  HELLO: 0,
  OPEN_CHANNEL: 1,
  CLOSE_CHANNEL: 2,
  CANCEL_CHANNEL: 3,
  GRANT_CREDITS: 4,
  PING: 5,
  PONG: 6,
  GO_AWAY: 7,
} as const;

/** Protocol version 1.0 */
export const PROTOCOL_VERSION_1_0 = 0x00010000;

/** Feature flags */
export const Features = {
  ATTACHED_STREAMS: 1n << 0n,
  CALL_ENVELOPE: 1n << 1n,
  CREDIT_FLOW_CONTROL: 1n << 2n,
  RAPACE_PING: 1n << 3n,
} as const;

/** Role in handshake */
export enum Role {
  Initiator = 1,
  Acceptor = 2,
}

/**
 * A frame with descriptor and optional external payload.
 */
export interface Frame {
  desc: MsgDescHot;
  payload: Uint8Array;
}

/**
 * Read a length-prefixed frame from a buffer.
 * Returns [frame, remainingBuffer] or null if not enough data.
 */
function readFrame(buffer: Buffer): [Frame, Buffer] | null {
  if (buffer.length < 4) return null;

  const totalLen = buffer.readUInt32LE(0);
  if (buffer.length < 4 + totalLen) return null;

  if (totalLen < MSG_DESC_HOT_SIZE) {
    throw new Error(`Frame too short: ${totalLen} bytes`);
  }

  const descData = buffer.subarray(4, 4 + MSG_DESC_HOT_SIZE);
  const desc = MsgDescHot.parse(new Uint8Array(descData));

  const payloadLen = totalLen - MSG_DESC_HOT_SIZE;
  const payload =
    payloadLen > 0
      ? new Uint8Array(buffer.subarray(4 + MSG_DESC_HOT_SIZE, 4 + totalLen))
      : new Uint8Array(0);

  const remaining = buffer.subarray(4 + totalLen);

  return [{ desc, payload }, remaining];
}

/**
 * Write a length-prefixed frame to a buffer.
 */
function writeFrame(frame: Frame): Buffer {
  const descBytes = frame.desc.serialize();
  const externalPayload =
    frame.desc.payloadSlot === INLINE_PAYLOAD_SLOT ? new Uint8Array(0) : frame.payload;

  const totalLen = MSG_DESC_HOT_SIZE + externalPayload.length;
  const buffer = Buffer.alloc(4 + totalLen);

  buffer.writeUInt32LE(totalLen, 0);
  buffer.set(descBytes, 4);
  if (externalPayload.length > 0) {
    buffer.set(externalPayload, 4 + MSG_DESC_HOT_SIZE);
  }

  return buffer;
}

/**
 * Create an inline frame.
 */
function inlineFrame(desc: MsgDescHot, payload: Uint8Array): Frame {
  if (payload.length > 16) {
    throw new Error("Payload too large for inline");
  }
  desc.setInlinePayload(payload);
  return { desc, payload: new Uint8Array(0) };
}

/**
 * Create a frame with external payload.
 */
function externalFrame(desc: MsgDescHot, payload: Uint8Array): Frame {
  desc.payloadSlot = 0;
  desc.payloadLen = payload.length;
  return { desc, payload };
}

/**
 * Simple postcard-like encoder for Hello message.
 * This is a minimal implementation for testing purposes.
 */
function encodeHello(hello: {
  protocolVersion: number;
  role: Role;
  requiredFeatures: bigint;
  supportedFeatures: bigint;
  limits: { maxPayloadSize: number; maxChannels: number; maxPendingCalls: number };
  methods: Array<{ methodId: number; sigHash: Uint8Array; name: string | null }>;
  params: Array<[string, Uint8Array]>;
}): Uint8Array {
  // This is a simplified encoder - real implementation would use proper postcard
  const parts: number[] = [];

  // protocol_version: u32
  parts.push(
    hello.protocolVersion & 0xff,
    (hello.protocolVersion >> 8) & 0xff,
    (hello.protocolVersion >> 16) & 0xff,
    (hello.protocolVersion >> 24) & 0xff
  );

  // role: u8
  parts.push(hello.role);

  // required_features: u64 (varint)
  let n = hello.requiredFeatures;
  while (n >= 0x80n) {
    parts.push(Number(n & 0x7fn) | 0x80);
    n >>= 7n;
  }
  parts.push(Number(n));

  // supported_features: u64 (varint)
  n = hello.supportedFeatures;
  while (n >= 0x80n) {
    parts.push(Number(n & 0x7fn) | 0x80);
    n >>= 7n;
  }
  parts.push(Number(n));

  // limits
  const limits = hello.limits;
  // max_payload_size: u32 (varint)
  let v = limits.maxPayloadSize;
  while (v >= 0x80) {
    parts.push((v & 0x7f) | 0x80);
    v >>= 7;
  }
  parts.push(v);

  // max_channels: u32 (varint)
  v = limits.maxChannels;
  while (v >= 0x80) {
    parts.push((v & 0x7f) | 0x80);
    v >>= 7;
  }
  parts.push(v);

  // max_pending_calls: u32 (varint)
  v = limits.maxPendingCalls;
  while (v >= 0x80) {
    parts.push((v & 0x7f) | 0x80);
    v >>= 7;
  }
  parts.push(v);

  // methods: Vec length (varint)
  parts.push(hello.methods.length);

  for (const method of hello.methods) {
    // method_id: u32 (varint)
    v = method.methodId;
    while (v >= 0x80) {
      parts.push((v & 0x7f) | 0x80);
      v >>= 7;
    }
    parts.push(v);

    // sig_hash: [u8; 32]
    for (let i = 0; i < 32; i++) {
      parts.push(method.sigHash[i] ?? 0);
    }

    // name: Option<String>
    if (method.name === null) {
      parts.push(0); // None
    } else {
      parts.push(1); // Some
      const nameBytes = new TextEncoder().encode(method.name);
      parts.push(nameBytes.length);
      parts.push(...nameBytes);
    }
  }

  // params: Vec length
  parts.push(hello.params.length);

  return new Uint8Array(parts);
}

/**
 * Conformance test runner.
 */
export class ConformanceRunner {
  private process: ChildProcess | null = null;
  private buffer: Buffer = Buffer.alloc(0);
  private frameQueue: Frame[] = [];
  private waiters: Array<(frame: Frame) => void> = [];

  /**
   * Start a conformance test case.
   */
  async start(testCase: string, conformanceBinary?: string): Promise<void> {
    const binary = conformanceBinary ?? "rapace-conformance";

    this.process = spawn(binary, ["--case", testCase], {
      stdio: ["pipe", "pipe", "inherit"],
    });

    this.process.stdout!.on("data", (data: Buffer) => {
      this.buffer = Buffer.concat([this.buffer, data]);
      this.processBuffer();
    });

    this.process.on("error", (err) => {
      console.error("Conformance process error:", err);
    });
  }

  private processBuffer(): void {
    while (true) {
      const result = readFrame(this.buffer);
      if (!result) break;

      const [frame, remaining] = result;
      this.buffer = remaining;

      const waiter = this.waiters.shift();
      if (waiter) {
        waiter(frame);
      } else {
        this.frameQueue.push(frame);
      }
    }
  }

  /**
   * Send a frame to the conformance harness.
   */
  send(frame: Frame): void {
    if (!this.process?.stdin) {
      throw new Error("Process not started");
    }
    const buffer = writeFrame(frame);
    this.process.stdin.write(buffer);
  }

  /**
   * Receive a frame from the conformance harness.
   */
  async recv(timeoutMs = 5000): Promise<Frame> {
    const queued = this.frameQueue.shift();
    if (queued) return queued;

    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        const idx = this.waiters.indexOf(resolve);
        if (idx >= 0) this.waiters.splice(idx, 1);
        reject(new Error("Timeout waiting for frame"));
      }, timeoutMs);

      this.waiters.push((frame) => {
        clearTimeout(timeout);
        resolve(frame);
      });
    });
  }

  /**
   * Wait for the conformance process to exit and return the result.
   */
  async finish(): Promise<{ passed: boolean; exitCode: number }> {
    if (!this.process) {
      throw new Error("Process not started");
    }

    // Close stdin to signal we're done
    this.process.stdin?.end();

    return new Promise((resolve) => {
      this.process!.on("close", (code) => {
        resolve({
          passed: code === 0,
          exitCode: code ?? -1,
        });
      });
    });
  }

  /**
   * Run a complete handshake as initiator.
   */
  async doHandshakeAsInitiator(): Promise<void> {
    // Send Hello as initiator
    const hello = encodeHello({
      protocolVersion: PROTOCOL_VERSION_1_0,
      role: Role.Initiator,
      requiredFeatures: 0n,
      supportedFeatures: Features.ATTACHED_STREAMS | Features.CALL_ENVELOPE,
      limits: { maxPayloadSize: 1024 * 1024, maxChannels: 0, maxPendingCalls: 0 },
      methods: [],
      params: [],
    });

    const desc = new MsgDescHot();
    desc.msgId = 1n;
    desc.channelId = 0;
    desc.methodId = ControlVerb.HELLO;
    desc.flags = FrameFlags.CONTROL;

    const frame = hello.length <= 16 ? inlineFrame(desc, hello) : externalFrame(desc, hello);

    this.send(frame);

    // Wait for Hello response
    const response = await this.recv();
    if (response.desc.channelId !== 0 || response.desc.methodId !== ControlVerb.HELLO) {
      throw new Error("Expected Hello response");
    }
  }
}

/**
 * Run a single conformance test.
 */
export async function runConformanceTest(
  testCase: string,
  testFn: (runner: ConformanceRunner) => Promise<void>,
  conformanceBinary?: string
): Promise<boolean> {
  const runner = new ConformanceRunner();

  try {
    await runner.start(testCase, conformanceBinary);
    await testFn(runner);
    const result = await runner.finish();
    return result.passed;
  } catch (err) {
    console.error(`Test ${testCase} failed:`, err);
    return false;
  }
}
