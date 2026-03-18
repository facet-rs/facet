import { createRequire } from "module";
import { fileURLToPath } from "url";
import path from "path";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const projectRoot = path.resolve(__dirname, "../..");

// We need to load from the built dist since node --experimental-transform-types
// doesn't resolve workspace packages from source.
// Load roam-wire dist directly.
const wireDist = path.join(projectRoot, "typescript/packages/roam-wire/dist/index.mjs");
const coreDist = path.join(projectRoot, "typescript/packages/roam-core/dist/index.mjs");

const wire = await import(wireDist);
const core = await import(coreDist);

console.error("wireMessageSchemasCbor type:", typeof wire.wireMessageSchemasCbor);
console.error("wireMessageSchemasCbor length:", wire.wireMessageSchemasCbor?.length);
console.error("wireMessageSchemasCbor first byte:", wire.wireMessageSchemasCbor?.[0]?.toString(16));

// Simulate what handshakeAsInitiator does by creating a fake link
// that captures the bytes sent.
const capturedFrames = [];
const fakeLink = {
  send: async (bytes) => {
    capturedFrames.push(new Uint8Array(bytes));
  },
  recv: async () => null,
  close: () => { },
  isClosed: () => false,
};

// session.initiatorOnLink does handshakeAsInitiator internally.
// We can't call it directly since it's not exported.
// Instead let's look at what the built dist contains for handshake.
// Check if handshakeAsInitiator is exported from core.
console.error("core exports:", Object.keys(core).filter(k => k.toLowerCase().includes("handshake")));

// Try calling session.initiatorOnLink with our fake link
try {
  const sessionResult = await core.session.initiatorOnLink(fakeLink, {});
} catch (e) {
  // Expected to fail since fakeLink.recv returns null
}

console.error("Captured frames:", capturedFrames.length);
if (capturedFrames.length > 0) {
  const frame = capturedFrames[0];
  console.error(
    "Frame 0 first 20 bytes:",
    Array.from(frame.slice(0, 20))
      .map((b) => "0x" + b.toString(16).padStart(2, "0"))
      .join(" "),
  );
  console.error("Frame 0 first byte major type:", frame[0] >> 5, "additional:", frame[0] & 0x1f);
  if (frame[0] === 0xa1) {
    console.error("✓ First byte is 0xA1 = 1-entry CBOR map (correct for HandshakeMessage enum)");
  } else {
    console.error("✗ First byte is NOT 0xA1, got:", "0x" + frame[0].toString(16));
  }
}
