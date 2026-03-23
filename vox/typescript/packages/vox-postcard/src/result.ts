export type VoxErrorPayload =
  | { tag: "User"; value: Uint8Array }
  | { tag: "UnknownMethod" }
  | { tag: "InvalidPayload" }
  | { tag: "Cancelled" };
