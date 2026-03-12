import type { Parity } from "@bearcove/roam-wire";
import { parityEven, parityOdd } from "@bearcove/roam-wire";
import { Role } from "../channeling/types.ts";

export function roleFromParity(parity: Parity): Role {
  return parity.tag === "Odd" ? Role.Initiator : Role.Acceptor;
}

export function firstIdForParity(parity: Parity): bigint {
  return parity.tag === "Odd" ? 1n : 2n;
}

export function parityFromRole(role: Role): Parity {
  return role === Role.Initiator ? parityOdd() : parityEven();
}

export function oppositeParity(parity: Parity): Parity {
  return parity.tag === "Odd" ? parityEven() : parityOdd();
}
