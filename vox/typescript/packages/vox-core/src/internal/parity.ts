import type { Parity } from "@bearcove/vox-wire";
import { parityEven, parityOdd } from "@bearcove/vox-wire";
import { Role } from "../channeling/types.ts";

export function roleFromParity(parity: Parity): Role {
  // r[impl connection.parity]
  return parity.tag === "Odd" ? Role.Initiator : Role.Acceptor;
}

export function firstIdForParity(parity: Parity): bigint {
  // r[impl session.parity]
  // r[impl connection.parity]
  return parity.tag === "Odd" ? 1n : 2n;
}

export function parityFromRole(role: Role): Parity {
  // r[impl session.parity]
  return role === Role.Initiator ? parityOdd() : parityEven();
}

export function oppositeParity(parity: Parity): Parity {
  // r[impl connection.parity]
  return parity.tag === "Odd" ? parityEven() : parityOdd();
}
