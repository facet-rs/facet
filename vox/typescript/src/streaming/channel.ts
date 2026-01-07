// Simple async channel for stream data.

/**
 * A simple multi-producer single-consumer async channel.
 *
 * This is similar to Rust's mpsc channel but for TypeScript.
 */
export interface Channel<T> {
  send(value: T): boolean;
  recv(): Promise<T | null>;
  close(): void;
  isClosed(): boolean;
}

interface ChannelState<T> {
  buffer: T[];
  closed: boolean;
  waiters: Array<(value: T | null) => void>;
}

/**
 * Create a new channel with the specified buffer capacity.
 */
export function createChannel<T>(capacity = 64): Channel<T> {
  const state: ChannelState<T> = {
    buffer: [],
    closed: false,
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
        waiter(value);
        return true;
      }

      // Buffer if under capacity
      if (state.buffer.length < capacity) {
        state.buffer.push(value);
        return true;
      }

      // Buffer full - drop (like try_send)
      return false;
    },

    async recv(): Promise<T | null> {
      // Return buffered value if available
      if (state.buffer.length > 0) {
        return state.buffer.shift()!;
      }

      // Channel closed and empty
      if (state.closed) {
        return null;
      }

      // Wait for a value
      return new Promise((resolve) => {
        state.waiters.push(resolve);
      });
    },

    close(): void {
      if (state.closed) return;
      state.closed = true;
      // Wake all waiters with null
      for (const waiter of state.waiters) {
        waiter(null);
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
  constructor(private channel: Channel<T>) {}

  send(value: T): boolean {
    return this.channel.send(value);
  }

  close(): void {
    this.channel.close();
  }
}

/**
 * Receiver end of a channel (for Pull).
 */
export class ChannelReceiver<T> {
  constructor(private channel: Channel<T>) {}

  recv(): Promise<T | null> {
    return this.channel.recv();
  }

  isClosed(): boolean {
    return this.channel.isClosed();
  }
}

/**
 * Create a sender/receiver pair.
 */
export function createChannelPair<T>(capacity = 64): [ChannelSender<T>, ChannelReceiver<T>] {
  const channel = createChannel<T>(capacity);
  return [new ChannelSender(channel), new ChannelReceiver(channel)];
}
