import { describe, expect, it } from "vitest";
import { InProcessLink } from "./transport.ts";

function inProcessPair(): [InProcessLink, InProcessLink] {
  let left!: InProcessLink;
  let right!: InProcessLink;
  left = new InProcessLink(
    (payload) => right.pushMessage(payload),
    () => right.pushClose(),
  );
  right = new InProcessLink(
    (payload) => left.pushMessage(payload),
    () => left.pushClose(),
  );
  return [left, right];
}

describe("InProcessLink", () => {
  // r[verify link]
  // r[verify link.message]
  // r[verify link.message.empty]
  // r[verify link.order]
  // r[verify link.rx.recv]
  // r[verify link.rx.eof]
  // r[verify link.tx.send]
  // r[verify link.tx.close]
  it("preserves owned message boundaries, order, empty payloads, and peer EOF", async () => {
    const [sender, receiver] = inProcessPair();

    const first = Uint8Array.of(1, 2);
    await sender.send(first);
    first[0] = 9;
    await sender.send(new Uint8Array(0));
    await sender.send(Uint8Array.of(3));
    sender.close();

    await expect(receiver.recv()).resolves.toEqual(Uint8Array.of(1, 2));
    await expect(receiver.recv()).resolves.toEqual(new Uint8Array(0));
    await expect(receiver.recv()).resolves.toEqual(Uint8Array.of(3));
    await expect(receiver.recv()).resolves.toBeNull();
    await expect(receiver.recv()).resolves.toBeNull();
    await expect(sender.send(Uint8Array.of(4))).rejects.toThrow("InProcessLink closed");
  });
});
