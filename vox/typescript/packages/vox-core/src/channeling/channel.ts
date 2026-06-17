// Simple async channel for stream data.

/**
 * A simple multi-producer single-consumer async channel.
 *
 * This is similar to Rust's mpsc channel but for TypeScript.
 */
export interface Channel<T> {
  send(value: T): boolean;
  recv(): Promise<T | null>;
  close(error?: Error): void;
  isClosed(): boolean;
}

interface ChannelState<T> {
  buffer: T[];
  closed: boolean;
  terminalError: Error | undefined;
  waiters: Array<{
    resolve: (value: T | null) => void;
    reject: (error: Error) => void;
  }>;
}

/**
 * Create a new unbounded channel.
 */
export function createChannel<T>(): Channel<T> {
  const state: ChannelState<T> = {
    buffer: [],
    closed: false,
    terminalError: undefined,
    waiters: [],
  };

  return {
    send(value: T): boolean {
      if (state.closed) {
        return false;
      }

      // If there's a waiter, deliver directly
      const waiter = state.waiters.shift();
      if (waiter) {
        waiter.resolve(value);
        return true;
      }

      state.buffer.push(value);
      return true;
    },

    async recv(): Promise<T | null> {
      // Return buffered value if available
      if (state.buffer.length > 0) {
        return state.buffer.shift()!;
      }

      // Channel closed and empty
      if (state.closed) {
        if (state.terminalError) {
          throw state.terminalError;
        }
        return null;
      }

      // Wait for a value
      return new Promise((resolve, reject) => {
        state.waiters.push({ resolve, reject });
      });
    },

    close(error?: Error): void {
      if (state.closed) return;
      state.closed = true;
      state.terminalError = error;
      // Wake all waiters with null
      for (const waiter of state.waiters) {
        if (error) {
          waiter.reject(error);
        } else {
          waiter.resolve(null);
        }
      }
      state.waiters.length = 0;
    },

    isClosed(): boolean {
      return state.closed;
    },
  };
}

/**
 * Sender end of a channel (for Push).
 */
export class ChannelSender<T> {
  private channel: Channel<T>;
  private _keepaliveOwner?: object;

  constructor(channel: Channel<T>, _keepaliveOwner?: object) {
    this.channel = channel;
    this._keepaliveOwner = _keepaliveOwner;
  }

  send(value: T): boolean {
    return this.channel.send(value);
  }

  close(error?: Error): void {
    this.channel.close(error);
  }
}

/**
 * Receiver end of a channel (for Pull).
 */
export class ChannelReceiver<T> {
  private channel: Channel<T>;
  private _keepaliveOwner?: object;
  private readonly onRecv?: () => void;

  constructor(channel: Channel<T>, _keepaliveOwner?: object, onRecv?: () => void) {
    this.channel = channel;
    this._keepaliveOwner = _keepaliveOwner;
    this.onRecv = onRecv;
  }

  async recv(): Promise<T | null> {
    const value = await this.channel.recv();
    if (value !== null) {
      this.onRecv?.();
    }
    return value;
  }

  isClosed(): boolean {
    return this.channel.isClosed();
  }
}
