/**
 * Postcard serialization module.
 *
 * Postcard is a compact binary format that uses:
 * - LEB128 varint encoding for variable-length integers
 * - Zigzag encoding for signed integers
 * - Length-prefixed strings and byte arrays
 * - Simple tag-based option encoding
 */

export {
  VarintError,
  ByteReader,
  encodeVarint,
  decodeVarint,
  decodeVarintNumber,
  zigzagEncode,
  zigzagDecode,
  encodeSignedVarint,
  decodeSignedVarint,
  decodeSignedVarintNumber,
} from "./varint.js";

export { PostcardEncoder, encode } from "./encoder.js";
export type { PostcardEncodable } from "./encoder.js";

export { PostcardDecoder, decode } from "./decoder.js";
export type { PostcardDecodable } from "./decoder.js";
