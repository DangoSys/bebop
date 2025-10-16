#include "socket.h"
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <unistd.h>
#include <cstring>
#include <cstdio>

SocketClient::SocketClient() 
  : sock_fd(-1), socket_initialized(false) {
}

SocketClient::~SocketClient() {
  close();
}

bool SocketClient::init() {
  if (socket_initialized) {
    return true;
  }
  
  sock_fd = socket(AF_INET, SOCK_STREAM, 0);
  if (sock_fd < 0) {
    fprintf(stderr, "Socket: Failed to create socket\n");
    return false;
  }
  
  struct sockaddr_in server_addr;
  memset(&server_addr, 0, sizeof(server_addr));
  server_addr.sin_family = AF_INET;
  server_addr.sin_port = htons(SOCKET_PORT);
  
  if (inet_pton(AF_INET, SOCKET_HOST, &server_addr.sin_addr) <= 0) {
    fprintf(stderr, "Socket: Invalid address/Address not supported\n");
    ::close(sock_fd);
    sock_fd = -1;
    return false;
  }
  
  if (connect(sock_fd, (struct sockaddr*)&server_addr, sizeof(server_addr)) < 0) {
    fprintf(stderr, "Socket: Connection failed to %s:%d\n", 
            SOCKET_HOST, SOCKET_PORT);
    ::close(sock_fd);
    sock_fd = -1;
    return false;
  }
  
  socket_initialized = true;
  printf("Socket: Connected to %s:%d\n", SOCKET_HOST, SOCKET_PORT);
  return true;
}

void SocketClient::close() {
  if (sock_fd >= 0) {
    ::close(sock_fd);
    sock_fd = -1;
  }
  socket_initialized = false;
}

bool SocketClient::send_request(const socket_msg_t& msg) {
  if (sock_fd < 0) {
    fprintf(stderr, "Socket: Not connected, cannot send request\n");
    return false;
  }
  
  ssize_t sent = send(sock_fd, &msg, sizeof(msg), 0);
  if (sent < 0) {
    fprintf(stderr, "Socket: Failed to send message\n");
    close();
    return false;
  }
  
  return true;
}

bool SocketClient::recv_response(socket_resp_t& resp) {
  if (sock_fd < 0) {
    fprintf(stderr, "Socket: Not connected, cannot receive response\n");
    return false;
  }
  
  ssize_t received = recv(sock_fd, &resp, sizeof(resp), 0);
  
  if (received < 0) {
    fprintf(stderr, "Socket: Failed to receive response\n");
    close();
    return false;
  } else if (received == 0) {
    fprintf(stderr, "Socket: Connection closed by remote\n");
    close();
    return false;
  }
  
  return true;
}

uint64_t SocketClient::send_and_wait(uint32_t funct, uint64_t xs1, uint64_t xs2) {
  // Auto-connect if not connected
  if (!socket_initialized) {
    if (!init()) {
      return 0;
    }
  }
  
  // Prepare and send request
  socket_msg_t msg;
  msg.funct = funct;
  msg.xs1 = xs1;
  msg.xs2 = xs2;
  
  if (!send_request(msg)) {
    return 0;
  }
  
  // Wait for response
  socket_resp_t resp;
  if (!recv_response(resp)) {
    return 0;
  }
  
  return resp.result;
}

