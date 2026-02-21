#include "roam_shm_atomic.h"

#include <errno.h>
#include <string.h>
#include <sys/socket.h>

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
