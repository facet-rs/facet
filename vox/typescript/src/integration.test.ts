/**
 * Integration tests against the rapace browser-tests-server.
 *
 * These tests require the server to be running:
 *   cd ../rapace && cargo run -p rapace-browser-tests-server
 *
 * Or set RAPACE_BROWSER_WS_PORT to use a different port.
 */

import { describe, it, before, after } from "node:test";
import * as assert from "node:assert";
import {
  RapaceClient,
  PostcardEncoder,
  PostcardDecoder,
  computeMethodId,
} from "./index.js";

// BrowserDemo service method IDs
const METHOD_SUMMARIZE_NUMBERS = computeMethodId("BrowserDemo", "summarize_numbers");
const METHOD_TRANSFORM_PHRASE = computeMethodId("BrowserDemo", "transform_phrase");

// Skip integration tests if SKIP_INTEGRATION is set
const SKIP_INTEGRATION = process.env.SKIP_INTEGRATION === "1";
const WS_PORT = process.env.RAPACE_BROWSER_WS_PORT || "4788";
const WS_URL = `ws://127.0.0.1:${WS_PORT}`;

// Types matching the Rust definitions
interface NumbersRequest {
  values: number[];
}

interface NumbersSummary {
  sum: bigint;
  mean: number;
  min: number;
  max: number;
}

interface PhraseRequest {
  phrase: string;
  shout: boolean;
}

interface PhraseResponse {
  title: string;
  originalLen: number;
}

// Encoder/decoder functions
function encodeNumbersRequest(encoder: PostcardEncoder, req: NumbersRequest): void {
  // Vec<i32> is encoded as: varint length, then each i32 as zigzag varint
  encoder.array(req.values, (enc, v) => enc.i32(v));
}

function decodeNumbersSummary(decoder: PostcardDecoder): NumbersSummary {
  return {
    sum: decoder.i64(),
    mean: decoder.f64(),
    min: decoder.i32(),
    max: decoder.i32(),
  };
}

function encodePhraseRequest(encoder: PostcardEncoder, req: PhraseRequest): void {
  encoder.string(req.phrase);
  encoder.bool(req.shout);
}

function decodePhraseResponse(decoder: PostcardDecoder): PhraseResponse {
  return {
    title: decoder.string(),
    originalLen: decoder.u32(),
  };
}

describe("Integration: BrowserDemo service", { skip: SKIP_INTEGRATION }, () => {
  let client: RapaceClient;

  before(async () => {
    try {
      client = await RapaceClient.connect(WS_URL);
    } catch (error) {
      console.error(`Failed to connect to ${WS_URL}. Is the server running?`);
      console.error("Start it with: cd ../rapace && cargo run -p rapace-browser-tests-server");
      throw error;
    }
  });

  after(() => {
    if (client) {
      client.close();
    }
  });

  it("summarize_numbers with positive values", async () => {
    const response = await client.callTyped<NumbersRequest, NumbersSummary>(
      METHOD_SUMMARIZE_NUMBERS,
      { values: [1, 2, 3, 4, 5] },
      encodeNumbersRequest,
      decodeNumbersSummary
    );

    assert.strictEqual(response.sum, 15n);
    assert.strictEqual(response.mean, 3.0);
    assert.strictEqual(response.min, 1);
    assert.strictEqual(response.max, 5);
  });

  it("summarize_numbers with negative values", async () => {
    const response = await client.callTyped<NumbersRequest, NumbersSummary>(
      METHOD_SUMMARIZE_NUMBERS,
      { values: [-10, 0, 10, 20] },
      encodeNumbersRequest,
      decodeNumbersSummary
    );

    assert.strictEqual(response.sum, 20n);
    assert.strictEqual(response.mean, 5.0);
    assert.strictEqual(response.min, -10);
    assert.strictEqual(response.max, 20);
  });

  it("summarize_numbers with empty array", async () => {
    const response = await client.callTyped<NumbersRequest, NumbersSummary>(
      METHOD_SUMMARIZE_NUMBERS,
      { values: [] },
      encodeNumbersRequest,
      decodeNumbersSummary
    );

    assert.strictEqual(response.sum, 0n);
    assert.strictEqual(response.mean, 0.0);
    assert.strictEqual(response.min, 0);
    assert.strictEqual(response.max, 0);
  });

  it("transform_phrase without shout", async () => {
    const response = await client.callTyped<PhraseRequest, PhraseResponse>(
      METHOD_TRANSFORM_PHRASE,
      { phrase: "hello world", shout: false },
      encodePhraseRequest,
      decodePhraseResponse
    );

    assert.strictEqual(response.title, "Hello World");
    assert.strictEqual(response.originalLen, 11);
  });

  it("transform_phrase with shout", async () => {
    const response = await client.callTyped<PhraseRequest, PhraseResponse>(
      METHOD_TRANSFORM_PHRASE,
      { phrase: "hello world", shout: true },
      encodePhraseRequest,
      decodePhraseResponse
    );

    assert.strictEqual(response.title, "HELLO WORLD");
    assert.strictEqual(response.originalLen, 11);
  });

  it("transform_phrase with unicode", async () => {
    const response = await client.callTyped<PhraseRequest, PhraseResponse>(
      METHOD_TRANSFORM_PHRASE,
      { phrase: "héllo wörld 你好", shout: false },
      encodePhraseRequest,
      decodePhraseResponse
    );

    // The char count should be correct for unicode
    assert.strictEqual(response.originalLen, 14); // h é l l o   w ö r l d   你 好
  });
});

describe("Method ID computation", () => {
  it("computes correct method IDs for BrowserDemo", () => {
    // These should match the Rust-computed values
    // We can verify by checking the server output or comparing with rapace-swift
    const summarize = computeMethodId("BrowserDemo", "summarize_numbers");
    const transform = computeMethodId("BrowserDemo", "transform_phrase");
    const countdown = computeMethodId("BrowserDemo", "countdown");

    // Method IDs should be non-zero 32-bit values
    assert.ok(summarize > 0 && summarize <= 0xffffffff, `summarize_numbers: ${summarize.toString(16)}`);
    assert.ok(transform > 0 && transform <= 0xffffffff, `transform_phrase: ${transform.toString(16)}`);
    assert.ok(countdown > 0 && countdown <= 0xffffffff, `countdown: ${countdown.toString(16)}`);

    // They should all be different
    assert.notStrictEqual(summarize, transform);
    assert.notStrictEqual(summarize, countdown);
    assert.notStrictEqual(transform, countdown);

    console.log("Method IDs:");
    console.log(`  BrowserDemo.summarize_numbers: 0x${summarize.toString(16).padStart(8, "0")}`);
    console.log(`  BrowserDemo.transform_phrase: 0x${transform.toString(16).padStart(8, "0")}`);
    console.log(`  BrowserDemo.countdown: 0x${countdown.toString(16).padStart(8, "0")}`);
  });
});
