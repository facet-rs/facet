/**
 * Frame flags carried in each message descriptor.
 *
 * These flags control frame handling and provide hints to the transport layer.
 */

export const FrameFlags = {
  /** Regular data frame. */
  DATA: 0b0000_0001,
  /** Control frame (channel 0). */
  CONTROL: 0b0000_0010,
  /** End of stream (half-close). */
  EOS: 0b0000_0100,
  /** Cancel this channel. */
  CANCEL: 0b0000_1000,
  /** Error response. */
  ERROR: 0b0001_0000,
  /** Priority scheduling hint. */
  HIGH_PRIORITY: 0b0010_0000,
  /** Contains credit grant. */
  CREDITS: 0b0100_0000,
  /** Headers/trailers only, no body. */
  METADATA_ONLY: 0b1000_0000,
  /** Don't send a reply frame for this request. */
  NO_REPLY: 0b0001_0000_0000,
} as const;

export type FrameFlagsType = number;

/**
 * Check if a flag is set.
 */
export function hasFlag(flags: FrameFlagsType, flag: number): boolean {
  return (flags & flag) !== 0;
}

/**
 * Set a flag.
 */
export function setFlag(flags: FrameFlagsType, flag: number): FrameFlagsType {
  return flags | flag;
}

/**
 * Clear a flag.
 */
export function clearFlag(flags: FrameFlagsType, flag: number): FrameFlagsType {
  return flags & ~flag;
}
