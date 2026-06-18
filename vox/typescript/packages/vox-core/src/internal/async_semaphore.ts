type Waiter = {
  resolve: (permit: AsyncSemaphorePermit) => void;
  reject: (error: Error) => void;
};

export class AsyncSemaphorePermit {
  private released = false;
  private readonly releaseFn: () => void;

  constructor(releaseFn: () => void) {
    this.releaseFn = releaseFn;
  }

  release(): void {
    if (this.released) {
      return;
    }
    this.released = true;
    this.releaseFn();
  }
}

// r[impl rpc.flow-control.max-concurrent-requests]
// r[impl rpc.flow-control.max-concurrent-requests.counting]
export class AsyncSemaphore {
  private permits: number;
  private readonly waiters: Waiter[] = [];
  private closed = false;
  private closeError: Error | null = null;

  constructor(permits: number) {
    this.permits = Math.max(0, permits);
  }

  acquire(): Promise<AsyncSemaphorePermit> {
    if (this.closed) {
      return Promise.reject(this.closeError ?? new Error("semaphore closed"));
    }

    if (this.permits > 0) {
      this.permits -= 1;
      return Promise.resolve(this.makePermit());
    }

    return new Promise((resolve, reject) => {
      if (this.closed) {
        reject(this.closeError ?? new Error("semaphore closed"));
        return;
      }
      this.waiters.push({ resolve, reject });
    });
  }

  close(error: Error = new Error("semaphore closed")): void {
    if (this.closed) {
      return;
    }
    this.closed = true;
    this.closeError = error;
    for (const waiter of this.waiters.splice(0)) {
      waiter.reject(error);
    }
  }

  debugSnapshot(): { availablePermits: number; waitingCount: number; closed: boolean } {
    return {
      availablePermits: this.permits,
      waitingCount: this.waiters.length,
      closed: this.closed,
    };
  }

  private makePermit(): AsyncSemaphorePermit {
    return new AsyncSemaphorePermit(() => this.release());
  }

  private release(): void {
    if (this.closed) {
      return;
    }
    const waiter = this.waiters.shift();
    if (waiter) {
      waiter.resolve(this.makePermit());
      return;
    }
    this.permits += 1;
  }
}
