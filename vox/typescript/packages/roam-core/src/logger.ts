// Opt-in logging for roam internals.
//
// Silent by default — nothing hits the console unless a logger is installed.
// Test harnesses can set `setRoamLogger(console)` to get full visibility.

export interface RoamLogger {
  debug(msg: string, ...args: unknown[]): void;
  error(msg: string, ...args: unknown[]): void;
}

let currentLogger: RoamLogger | null = null;

export function setRoamLogger(logger: RoamLogger | null): void {
  currentLogger = logger;
}

export function roamLogger(): RoamLogger | null {
  return currentLogger;
}
