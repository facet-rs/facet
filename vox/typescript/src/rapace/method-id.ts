/**
 * Method ID computation using FNV-1a hash.
 *
 * Method IDs are computed as 64-bit FNV-1a hashes of "Service.method",
 * then folded to 32 bits via XOR.
 */

const FNV_OFFSET_BASIS = 0xcbf29ce484222325n;
const FNV_PRIME = 0x100000001b3n;

/**
 * Compute a rapace method ID from service and method names.
 *
 * @param service - The service name
 * @param method - The method name
 * @returns The 32-bit method ID
 */
export function computeMethodId(service: string, method: string): number {
  const fullName = `${service}.${method}`;

  // FNV-1a 64-bit
  let hash = FNV_OFFSET_BASIS;

  const encoder = new TextEncoder();
  const bytes = encoder.encode(fullName);

  for (const byte of bytes) {
    hash ^= BigInt(byte);
    hash = BigInt.asUintN(64, hash * FNV_PRIME);
  }

  // Fold to 32 bits via XOR
  const folded = (hash >> 32n) ^ (hash & 0xffffffffn);
  return Number(folded);
}

/**
 * Compute a method ID from a full method name (e.g., "Vfs.read").
 *
 * @param fullName - The full method name in "Service.method" format
 * @returns The 32-bit method ID
 */
export function computeMethodIdFromFullName(fullName: string): number {
  const parts = fullName.split(".");
  if (parts.length !== 2) {
    throw new Error(`Invalid method name format: ${fullName}, expected "Service.method"`);
  }
  return computeMethodId(parts[0], parts[1]);
}
