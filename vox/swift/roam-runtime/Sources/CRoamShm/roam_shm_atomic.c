#include "roam_shm_atomic.h"

#include <stdatomic.h>

uint32_t roam_bipbuf_header_size(void) { return (uint32_t)sizeof(roam_bipbuf_header_t); }

void roam_bipbuf_init(roam_bipbuf_header_t *header, uint32_t capacity) {
  atomic_store_explicit(&header->write, 0, memory_order_release);
  atomic_store_explicit(&header->watermark, 0, memory_order_release);
  header->capacity = capacity;
  atomic_store_explicit(&header->read, 0, memory_order_release);
}

uint32_t roam_bipbuf_capacity(const roam_bipbuf_header_t *header) { return header->capacity; }

uint32_t roam_bipbuf_load_write_acquire(const roam_bipbuf_header_t *header) {
  return atomic_load_explicit(&header->write, memory_order_acquire);
}

uint32_t roam_bipbuf_load_read_acquire(const roam_bipbuf_header_t *header) {
  return atomic_load_explicit(&header->read, memory_order_acquire);
}

uint32_t roam_bipbuf_load_watermark_acquire(const roam_bipbuf_header_t *header) {
  return atomic_load_explicit(&header->watermark, memory_order_acquire);
}

int roam_bipbuf_try_grant(roam_bipbuf_header_t *header, uint32_t len, uint32_t *offset) {
  if (len == 0) {
    *offset = 0;
    return 1;
  }
  uint32_t capacity = header->capacity;
  if (len > capacity) {
    return -1;
  }

  uint32_t write = atomic_load_explicit(&header->write, memory_order_relaxed);
  uint32_t read = atomic_load_explicit(&header->read, memory_order_acquire);

  if (write >= read) {
    uint32_t space_at_end = capacity - write;
    if (space_at_end >= len) {
      *offset = write;
      return 1;
    }

    if (read == 0) {
      return 0;
    }

    atomic_store_explicit(&header->watermark, write, memory_order_release);
    atomic_store_explicit(&header->write, 0, memory_order_release);

    if (len < read) {
      *offset = 0;
      return 1;
    }

    atomic_store_explicit(&header->write, write, memory_order_release);
    atomic_store_explicit(&header->watermark, 0, memory_order_release);
    return 0;
  } else {
    if (write + len < read) {
      *offset = write;
      return 1;
    }
    return 0;
  }
}

int roam_bipbuf_commit(roam_bipbuf_header_t *header, uint32_t len) {
  uint32_t write = atomic_load_explicit(&header->write, memory_order_relaxed);
  uint32_t new_write = write + len;
  if (new_write < write || new_write > header->capacity) {
    return -1;
  }
  atomic_store_explicit(&header->write, new_write, memory_order_release);
  return 0;
}

int roam_bipbuf_try_read(roam_bipbuf_header_t *header, uint32_t *offset, uint32_t *len) {
  uint32_t read = atomic_load_explicit(&header->read, memory_order_relaxed);
  uint32_t watermark = atomic_load_explicit(&header->watermark, memory_order_acquire);

  // If wrap is active, consume tail [read..watermark) first.
  if (watermark != 0) {
    if (read < watermark) {
      *offset = read;
      *len = watermark - read;
      return 1;
    }

    // Consumed the tail; wrap consumer to front and clear watermark.
    atomic_store_explicit(&header->read, 0, memory_order_release);
    atomic_store_explicit(&header->watermark, 0, memory_order_release);
    read = 0;
  }

  uint32_t write = atomic_load_explicit(&header->write, memory_order_acquire);
  if (read < write) {
    *offset = read;
    *len = write - read;
    return 1;
  }

  return 0;
}

int roam_bipbuf_release(roam_bipbuf_header_t *header, uint32_t len) {
  uint32_t read = atomic_load_explicit(&header->read, memory_order_relaxed);
  uint32_t new_read = read + len;
  if (new_read < read || new_read > header->capacity) {
    return -1;
  }

  uint32_t watermark = atomic_load_explicit(&header->watermark, memory_order_acquire);
  if (watermark != 0 && new_read >= watermark) {
    atomic_store_explicit(&header->read, 0, memory_order_release);
    atomic_store_explicit(&header->watermark, 0, memory_order_release);
  } else {
    atomic_store_explicit(&header->read, new_read, memory_order_release);
  }

  return 0;
}

uint32_t roam_atomic_load_u32_relaxed(const uint32_t *ptr) {
  const _Atomic(uint32_t) *a = (const _Atomic(uint32_t) *)ptr;
  return atomic_load_explicit(a, memory_order_relaxed);
}

uint32_t roam_atomic_load_u32_acquire(const uint32_t *ptr) {
  const _Atomic(uint32_t) *a = (const _Atomic(uint32_t) *)ptr;
  return atomic_load_explicit(a, memory_order_acquire);
}

void roam_atomic_store_u32_release(uint32_t *ptr, uint32_t value) {
  _Atomic(uint32_t) *a = (_Atomic(uint32_t) *)ptr;
  atomic_store_explicit(a, value, memory_order_release);
}

uint64_t roam_atomic_load_u64_relaxed(const uint64_t *ptr) {
  const _Atomic(uint64_t) *a = (const _Atomic(uint64_t) *)ptr;
  return atomic_load_explicit(a, memory_order_relaxed);
}

uint64_t roam_atomic_load_u64_acquire(const uint64_t *ptr) {
  const _Atomic(uint64_t) *a = (const _Atomic(uint64_t) *)ptr;
  return atomic_load_explicit(a, memory_order_acquire);
}

void roam_atomic_store_u64_release(uint64_t *ptr, uint64_t value) {
  _Atomic(uint64_t) *a = (_Atomic(uint64_t) *)ptr;
  atomic_store_explicit(a, value, memory_order_release);
}
