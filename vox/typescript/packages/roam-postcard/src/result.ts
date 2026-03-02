export type RoamErrorPayload =
  | { tag: "User"; value: Uint8Array }
  | { tag: "UnknownMethod" }
  | { tag: "InvalidPayload" }
  | { tag: "Cancelled" };
