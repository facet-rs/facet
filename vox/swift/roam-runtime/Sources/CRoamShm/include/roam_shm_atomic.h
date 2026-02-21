#ifndef ROAM_SHM_ATOMIC_H
#define ROAM_SHM_ATOMIC_H

#include <stdint.h>
#include <sys/types.h>

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
