#include "ipc/socket.h"
#include <cstdio>
#include <cerrno>
#include <sys/socket.h>

// CMD path: send command request
bool SocketClient::send_cmd_request(const cmd_req_t &req) {
  if (cmd_sock_fd < 0) {
    fprintf(stderr, "Socket: Not connected, cannot send CMD request\n");
    return false;
  }

  // fprintf(stderr, "Socket: Sending CMD request: sizeof(req)=%zu, funct=%u\n",
  //         sizeof(req), req.funct);
  ssize_t sent = send(cmd_sock_fd, &req, sizeof(req), 0);
  if (sent < 0) {
    fprintf(stderr, "Socket: Failed to send CMD request\n");
    close();
    return false;
  }
  // fprintf(stderr, "Socket: Sent %zd bytes\n", sent);

  return true;
}

// CMD path: receive command response
bool SocketClient::recv_cmd_response(cmd_resp_t &resp) {
  if (cmd_sock_fd < 0) {
    fprintf(stderr, "Socket: Not connected, cannot receive CMD response\n");
    return false;
  }

  ssize_t received = recv(cmd_sock_fd, &resp, sizeof(resp), 0);

  if (received < 0) {
    fprintf(stderr, "Socket: Failed to receive CMD response\n");
    close();
    return false;
  } else if (received == 0) {
    fprintf(stderr, "Socket: CMD connection closed by remote\n");
    close();
    return false;
  } else if (received < (ssize_t)sizeof(resp)) {
    fprintf(stderr, "Socket: Incomplete CMD response (received %ld bytes, expected %lu bytes)\n", received, sizeof(resp));
    close();
    return false;
  }

  return true;
}
