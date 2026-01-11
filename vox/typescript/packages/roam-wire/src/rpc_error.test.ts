// Basic tests for RpcError

import { describe, it, expect } from "vitest";
import { RpcError, RpcErrorCode } from "./rpc_error.ts";

describe("RpcErrorCode", () => {
  it("has correct discriminant values", () => {
    expect(RpcErrorCode.USER).toBe(0);
    expect(RpcErrorCode.UNKNOWN_METHOD).toBe(1);
    expect(RpcErrorCode.INVALID_PAYLOAD).toBe(2);
    expect(RpcErrorCode.CANCELLED).toBe(3);
  });
});

describe("RpcError", () => {
  it("creates user error", () => {
    const payload = new Uint8Array([1, 2, 3]);
    const error = new RpcError(RpcErrorCode.USER, payload);

    expect(error.code).toBe(RpcErrorCode.USER);
    expect(error.payload).toBe(payload);
    expect(error.isUserError()).toBe(true);
    expect(error.isProtocolError()).toBe(false);
    expect(error.message).toBe("Application error");
  });

  it("creates unknown method error", () => {
    const error = new RpcError(RpcErrorCode.UNKNOWN_METHOD);

    expect(error.code).toBe(RpcErrorCode.UNKNOWN_METHOD);
    expect(error.payload).toBeNull();
    expect(error.isUserError()).toBe(false);
    expect(error.isProtocolError()).toBe(true);
    expect(error.message).toBe("Unknown method");
  });

  it("creates invalid payload error", () => {
    const error = new RpcError(RpcErrorCode.INVALID_PAYLOAD);

    expect(error.code).toBe(RpcErrorCode.INVALID_PAYLOAD);
    expect(error.payload).toBeNull();
    expect(error.isUserError()).toBe(false);
    expect(error.isProtocolError()).toBe(true);
    expect(error.message).toBe("Invalid payload");
  });

  it("creates cancelled error", () => {
    const error = new RpcError(RpcErrorCode.CANCELLED);

    expect(error.code).toBe(RpcErrorCode.CANCELLED);
    expect(error.payload).toBeNull();
    expect(error.isUserError()).toBe(false);
    expect(error.isProtocolError()).toBe(true);
    expect(error.message).toBe("Cancelled");
  });

  it("is an Error instance", () => {
    const error = new RpcError(RpcErrorCode.USER);
    expect(error).toBeInstanceOf(Error);
    expect(error.name).toBe("RpcError");
  });
});
