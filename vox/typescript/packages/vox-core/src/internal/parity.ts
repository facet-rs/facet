import type { Parity } from "@bearcove/vox-wire";
import { parityEven, parityOdd } from "@bearcove/vox-wire";
import { Role } from "../channeling/types.ts";

export function roleFromParity(parity: Parity): Role {
  // r[impl lane.request-channel-parity]
  return parity.tag === "Odd" ? Role.Initiator : Role.Acceptor;
}

export function firstIdForParity(parity: Parity): bigint {
  // r[impl connection.lane-id-parity]
  // r[impl lane.request-channel-parity]
  return parity.tag === "Odd" ? 1n : 2n;
}

export function parityFromRole(role: Role): Parity {
  // r[impl connection.lane-id-parity]
  return role === Role.Initiator ? parityOdd() : parityEven();
}

export function oppositeParity(parity: Parity): Parity {
  // r[impl lane.request-channel-parity]
  return parity.tag === "Odd" ? parityEven() : parityOdd();
}
