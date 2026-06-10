// Opt-in logging for vox internals.
//
// Silent by default — nothing hits the console unless a logger is installed.
// Test harnesses can set `setVoxLogger(console)` to get full visibility.

export interface VoxLogger {
  debug(msg: string, ...args: unknown[]): void;
  error(msg: string, ...args: unknown[]): void;
}

let currentLogger: VoxLogger | null = null;

// r[impl rpc.observability.runtime]
// r[impl rpc.observability.driver]
// r[impl rpc.observability.channel]
// r[impl rpc.observability.session-errors]
export function setVoxLogger(logger: VoxLogger | null): void {
  currentLogger = logger;
}

// r[impl rpc.observability.runtime]
export function voxLogger(): VoxLogger | null {
  return currentLogger;
}
