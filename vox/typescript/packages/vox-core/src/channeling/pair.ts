// Unbound channel pair creation.

import { Tx } from "./tx.ts";
import { Rx } from "./rx.ts";

/**
 * Create an unbound channel pair.
 *
 * The returned Tx and Rx are linked together but not yet bound to a channel ID.
 * They will be bound when the Rx is passed to a method call (the runtime binder
 * allocates a channel ID and binds both ends).
 *
 * Usage:
 * ```typescript
 * const [tx, rx] = channel<number>();
 *
 * // Start the call first (this binds the channels)
 * const resultPromise = client.sum(rx);
 *
 * // Now we can send data
 * await tx.send(1);
 * await tx.send(2);
 * tx.close();
 *
 * const result = await resultPromise;
 * ```
 *
 * r[impl rpc.channel]
 * r[impl rpc.channel.direction]
 * r[impl rpc.channel.pair]
 * r[impl rpc.channel.pair.binding-propagation]
 */
export function channel<T>(): [Tx<T>, Rx<T>] {
  const tx = new Tx<T>();
  const rx = new Rx<T>();

  // Link the pair together
  tx._pair = rx;
  rx._pair = tx;

  return [tx, rx];
}
