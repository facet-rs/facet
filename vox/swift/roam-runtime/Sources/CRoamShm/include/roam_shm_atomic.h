#ifndef ROAM_SHM_ATOMIC_H
#define ROAM_SHM_ATOMIC_H

#include <stdatomic.h>
#include <stdint.h>
#include <sys/types.h>

typedef struct roam_bipbuf_header_t {
  _Atomic(uint32_t) write;
  _Atomic(uint32_t) watermark;
  uint32_t capacity;
  uint8_t pad0[52];
  _Atomic(uint32_t) read;
  uint8_t pad1[60];
} roam_bipbuf_header_t;

uint32_t roam_bipbuf_header_size(void);
void roam_bipbuf_init(roam_bipbuf_header_t *header, uint32_t capacity);
uint32_t roam_bipbuf_capacity(const roam_bipbuf_header_t *header);
uint32_t roam_bipbuf_load_write_acquire(const roam_bipbuf_header_t *header);
uint32_t roam_bipbuf_load_read_acquire(const roam_bipbuf_header_t *header);
uint32_t roam_bipbuf_load_watermark_acquire(const roam_bipbuf_header_t *header);
int roam_bipbuf_try_grant(roam_bipbuf_header_t *header, uint32_t len, uint32_t *offset);
int roam_bipbuf_commit(roam_bipbuf_header_t *header, uint32_t len);
int roam_bipbuf_try_read(roam_bipbuf_header_t *header, uint32_t *offset, uint32_t *len);
int roam_bipbuf_release(roam_bipbuf_header_t *header, uint32_t len);

uint32_t roam_atomic_load_u32_relaxed(const uint32_t *ptr);
uint32_t roam_atomic_load_u32_acquire(const uint32_t *ptr);
void roam_atomic_store_u32_release(uint32_t *ptr, uint32_t value);
int roam_atomic_compare_exchange_u32(uint32_t *ptr, uint32_t *expected, uint32_t desired);
uint32_t roam_atomic_fetch_add_u32(uint32_t *ptr, uint32_t value);

uint64_t roam_atomic_load_u64_relaxed(const uint64_t *ptr);
uint64_t roam_atomic_load_u64_acquire(const uint64_t *ptr);
void roam_atomic_store_u64_release(uint64_t *ptr, uint64_t value);
int roam_atomic_compare_exchange_u64(uint64_t *ptr, uint64_t *expected, uint64_t desired);

// Receive one fd over a Unix domain socket using SCM_RIGHTS.
// Returns 1 on success and stores fd in out_fd.
// Returns 0 on EOF.
// Returns -1 on error (errno is set).
int roam_recv_one_fd(int sockfd, int *out_fd);

// Receive up to `max_fds` fds from one SCM_RIGHTS message.
// Returns number of fds received (>=1), 0 on EOF, -1 on error.
int roam_recv_fds(int sockfd, int *out_fds, int max_fds);

// Send `num_fds` file descriptors over a Unix domain socket using one
// SCM_RIGHTS message.
// Returns number of payload bytes sent (>0) on success, -1 on error.
int roam_send_fds(int sockfd, const int *fds, int num_fds);

#endif
