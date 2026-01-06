declare module "node:net" {
  export interface Socket {
    write(data: Uint8Array): void;
    end(): void;
    on(event: "error", listener: (err: Error) => void): this;
    on(event: "data", listener: (chunk: Buffer) => void): this;
    on(event: "close", listener: () => void): this;
  }

  export function createConnection(
    options: { host: string; port: number },
    listener: () => void,
  ): Socket;
}

declare const process: {
  env: Record<string, string | undefined>;
  exit(code?: number): never;
};

declare class Buffer extends Uint8Array {
  static alloc(size: number): Buffer;
  static concat(list: readonly Uint8Array[]): Buffer;
  indexOf(value: number): number;
  subarray(start?: number, end?: number): Buffer;
}

