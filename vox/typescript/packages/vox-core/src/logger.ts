// Opt-in logging for vox internals.
//
// Silent by default — nothing hits the console unless a logger is installed.
// Test harnesses can set `setVoxLogger(console)` to get full visibility.

export interface VoxLogger {
  debug(msg: string, ...args: unknown[]): void;
  error(msg: string, ...args: unknown[]): void;
}

let currentLogger: VoxLogger | null = null;

export function setVoxLogger(logger: VoxLogger | null): void {
  currentLogger = logger;
}

export function voxLogger(): VoxLogger | null {
  return currentLogger;
}
