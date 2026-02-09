#include "roam_shm_atomic.h"

#include <errno.h>
#include <stdatomic.h>
#include <string.h>
#include <sys/socket.h>

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

int roam_atomic_compare_exchange_u32(uint32_t *ptr, uint32_t *expected, uint32_t desired) {
  _Atomic(uint32_t) *a = (_Atomic(uint32_t) *)ptr;
  return atomic_compare_exchange_weak_explicit(a, expected, desired, memory_order_acq_rel, memory_order_acquire);
}

uint32_t roam_atomic_fetch_add_u32(uint32_t *ptr, uint32_t value) {
  _Atomic(uint32_t) *a = (_Atomic(uint32_t) *)ptr;
  return atomic_fetch_add_explicit(a, value, memory_order_acq_rel);
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

int roam_atomic_compare_exchange_u64(uint64_t *ptr, uint64_t *expected, uint64_t desired) {
  _Atomic(uint64_t) *a = (_Atomic(uint64_t) *)ptr;
  return atomic_compare_exchange_weak_explicit(a, expected, desired, memory_order_acq_rel, memory_order_acquire);
}

int roam_recv_one_fd(int sockfd, int *out_fd) {
  int fds[1];
  int rc = roam_recv_fds(sockfd, fds, 1);
  if (rc <= 0) {
    return rc;
  }
  *out_fd = fds[0];
  return 1;
}

int roam_recv_fds(int sockfd, int *out_fds, int max_fds) {
  if (out_fds == NULL || max_fds <= 0) {
    errno = EINVAL;
    return -1;
  }
  if (max_fds > 8) {
    errno = EOVERFLOW;
    return -1;
  }

  unsigned char byte = 0;
  struct iovec iov = {
      .iov_base = &byte,
      .iov_len = 1,
  };

  unsigned char cmsgbuf[CMSG_SPACE(sizeof(int) * 8)];
  memset(cmsgbuf, 0, sizeof(cmsgbuf));

  struct msghdr msg;
  memset(&msg, 0, sizeof(msg));
  msg.msg_iov = &iov;
  msg.msg_iovlen = 1;
  msg.msg_control = cmsgbuf;
  msg.msg_controllen = sizeof(cmsgbuf);

  ssize_t n = recvmsg(sockfd, &msg, 0);
  if (n == 0) {
    return 0;
  }
  if (n < 0) {
    return -1;
  }

  if ((msg.msg_flags & MSG_CTRUNC) != 0) {
    errno = EMSGSIZE;
    return -1;
  }

  for (struct cmsghdr *cmsg = CMSG_FIRSTHDR(&msg); cmsg != NULL;
       cmsg = CMSG_NXTHDR(&msg, cmsg)) {
    if (cmsg->cmsg_level != SOL_SOCKET || cmsg->cmsg_type != SCM_RIGHTS) {
      continue;
    }

    size_t data_len = cmsg->cmsg_len - CMSG_LEN(0);
    if (data_len < sizeof(int)) {
      continue;
    }

    int *fds = (int *)CMSG_DATA(cmsg);
    int count = (int)(data_len / sizeof(int));
    if (count > max_fds) {
      errno = EOVERFLOW;
      return -1;
    }
    memcpy(out_fds, fds, (size_t)count * sizeof(int));
    return count;
  }

  errno = ENOMSG;
  return -1;
}

int roam_send_fds(int sockfd, const int *fds, int num_fds) {
  if (fds == NULL || num_fds <= 0) {
    errno = EINVAL;
    return -1;
  }
  if (num_fds > 8) {
    errno = EOVERFLOW;
    return -1;
  }

  unsigned char byte = 1;
  struct iovec iov = {
      .iov_base = &byte,
      .iov_len = 1,
  };

  unsigned char cmsgbuf[CMSG_SPACE(sizeof(int) * 8)];
  memset(cmsgbuf, 0, sizeof(cmsgbuf));

  struct msghdr msg;
  memset(&msg, 0, sizeof(msg));
  msg.msg_iov = &iov;
  msg.msg_iovlen = 1;
  msg.msg_control = cmsgbuf;
  msg.msg_controllen = CMSG_SPACE(sizeof(int) * (size_t)num_fds);

  struct cmsghdr *cmsg = CMSG_FIRSTHDR(&msg);
  if (cmsg == NULL) {
    errno = EIO;
    return -1;
  }
  cmsg->cmsg_level = SOL_SOCKET;
  cmsg->cmsg_type = SCM_RIGHTS;
  cmsg->cmsg_len = CMSG_LEN(sizeof(int) * (size_t)num_fds);
  memcpy(CMSG_DATA(cmsg), fds, sizeof(int) * (size_t)num_fds);

  while (1) {
    ssize_t n = sendmsg(sockfd, &msg, 0);
    if (n >= 0) {
      return (int)n;
    }
    if (errno == EINTR) {
      continue;
    }
    return -1;
  }
}
