#include "ipc/socket.h"
#include <arpa/inet.h>
#include <cstdio>
#include <cstring>
#include <netinet/in.h>
#include <sys/socket.h>
#include <unistd.h>
#include <thread>
#include <chrono>
#include <cerrno>
#include <errno.h>

SocketClient::SocketClient() : cmd_sock_fd(-1), dma_read_sock_fd(-1), dma_write_sock_fd(-1), socket_initialized(false), dma_handler_running(false) {}

SocketClient::~SocketClient() { close(); }

bool SocketClient::init() {
  if (socket_initialized) {
    return true;
  }

  // printf("Socket: Initializing connections...\n");
  fflush(stdout);
  
  // Connect to CMD socket
  cmd_sock_fd = socket(AF_INET, SOCK_STREAM, 0);
  if (cmd_sock_fd < 0) {
    printf("Socket: Failed to create CMD socket\n");
    fflush(stdout);
    return false;
  }

  struct sockaddr_in server_addr;
  memset(&server_addr, 0, sizeof(server_addr));
  server_addr.sin_family = AF_INET;
  server_addr.sin_port = htons(SOCKET_CMD_PORT);

  if (inet_pton(AF_INET, SOCKET_HOST, &server_addr.sin_addr) <= 0) {
    printf("Socket: Invalid address/Address not supported\n");
    fflush(stdout);
    ::close(cmd_sock_fd);
    cmd_sock_fd = -1;
    return false;
  }
  
  // printf("Socket: Attempting to connect to CMD socket %s:%d...\n", SOCKET_HOST, SOCKET_CMD_PORT);
  fflush(stdout);

  if (connect(cmd_sock_fd, (struct sockaddr *)&server_addr, sizeof(server_addr)) < 0) {
    printf("Socket: CMD connection failed to %s:%d\n", SOCKET_HOST, SOCKET_CMD_PORT);
    fflush(stdout);
    ::close(cmd_sock_fd);
    cmd_sock_fd = -1;
    return false;
  }

  // printf("Socket: Connected to CMD socket %s:%d\n", SOCKET_HOST, SOCKET_CMD_PORT);
  fflush(stdout);

  // Connect to DMA read socket
  dma_read_sock_fd = socket(AF_INET, SOCK_STREAM, 0);
  if (dma_read_sock_fd < 0) {
    printf("Socket: Failed to create DMA read socket\n");
    fflush(stdout);
    ::close(cmd_sock_fd);
    cmd_sock_fd = -1;
    return false;
  }

  server_addr.sin_port = htons(SOCKET_DMA_READ_PORT);
  // printf("Socket: Attempting to connect to DMA read socket %s:%d...\n", SOCKET_HOST, SOCKET_DMA_READ_PORT);
  fflush(stdout);
  if (connect(dma_read_sock_fd, (struct sockaddr *)&server_addr, sizeof(server_addr)) < 0) {
    printf("Socket: DMA read connection failed to %s:%d: %s\n", SOCKET_HOST, SOCKET_DMA_READ_PORT, strerror(errno));
    fflush(stdout);
    ::close(cmd_sock_fd);
    ::close(dma_read_sock_fd);
    cmd_sock_fd = -1;
    dma_read_sock_fd = -1;
    return false;
  }

  // printf("Socket: Connected to DMA read socket %s:%d\n", SOCKET_HOST, SOCKET_DMA_READ_PORT);
  fflush(stdout);

  // Connect to DMA write socket
  dma_write_sock_fd = socket(AF_INET, SOCK_STREAM, 0);
  if (dma_write_sock_fd < 0) {
    printf("Socket: Failed to create DMA write socket\n");
    fflush(stdout);
    ::close(cmd_sock_fd);
    ::close(dma_read_sock_fd);
    cmd_sock_fd = -1;
    dma_read_sock_fd = -1;
    return false;
  }

  server_addr.sin_port = htons(SOCKET_DMA_WRITE_PORT);
  // printf("Socket: Attempting to connect to DMA write socket %s:%d...\n", SOCKET_HOST, SOCKET_DMA_WRITE_PORT);
  fflush(stdout);
  if (connect(dma_write_sock_fd, (struct sockaddr *)&server_addr, sizeof(server_addr)) < 0) {
    printf("Socket: DMA write connection failed to %s:%d: %s\n", SOCKET_HOST, SOCKET_DMA_WRITE_PORT, strerror(errno));
    fflush(stdout);
    ::close(cmd_sock_fd);
    ::close(dma_read_sock_fd);
    ::close(dma_write_sock_fd);
    cmd_sock_fd = -1;
    dma_read_sock_fd = -1;
    dma_write_sock_fd = -1;
    return false;
  }

  // printf("Socket: Connected to DMA write socket %s:%d\n", SOCKET_HOST, SOCKET_DMA_WRITE_PORT);
  fflush(stdout);

  socket_initialized = true;
  
  // Start DMA handler thread
  start_dma_handler();
  
  return true;
}

void SocketClient::close() {
  dma_handler_running = false;
  if (cmd_sock_fd >= 0) {
    ::close(cmd_sock_fd);
    cmd_sock_fd = -1;
  }
  if (dma_read_sock_fd >= 0) {
    ::close(dma_read_sock_fd);
    dma_read_sock_fd = -1;
  }
  if (dma_write_sock_fd >= 0) {
    ::close(dma_write_sock_fd);
    dma_write_sock_fd = -1;
  }
  socket_initialized = false;
}

void SocketClient::set_dma_callbacks(dma_read_cb_t read_cb,
                                     dma_write_cb_t write_cb) {
  dma_read_cb = std::move(read_cb);
  dma_write_cb = std::move(write_cb);
}

// Receive message header (peek first to get type) - only used for CMD socket
bool SocketClient::recv_header(msg_header_t &header) {
  if (cmd_sock_fd < 0) {
    fprintf(stderr, "Socket: Not connected\n");
    return false;
  }

  ssize_t received = recv(cmd_sock_fd, &header, sizeof(header), MSG_PEEK);

  if (received < 0) {
    fprintf(stderr, "Socket: Failed to peek header\n");
    close();
    return false;
  } else if (received == 0) {
    fprintf(stderr, "Socket: Connection closed by remote\n");
    close();
    return false;
  }

  return true;
}

uint64_t SocketClient::send_and_wait(uint32_t funct, uint64_t xs1,
                                     uint64_t xs2) {
  // Auto-connect if not connected
  if (!socket_initialized) {
    if (!init()) {
      return 0;
    }
  }

  // Prepare and send CMD request
  cmd_req_t cmd_req;
  cmd_req.header.msg_type = MSG_TYPE_CMD_REQ;
  cmd_req.header.reserved = 0;
  cmd_req.funct = funct;
  cmd_req.padding = 0;
  cmd_req.xs1 = xs1;
  cmd_req.xs2 = xs2;

  if (!send_cmd_request(cmd_req)) {
    return 0;
  }

  // Now wait for CMD response (DMA requests are handled by separate thread)
  cmd_resp_t cmd_resp;
  if (!recv_cmd_response(cmd_resp)) {
    return 0;
  }
  return cmd_resp.result;
}

void SocketClient::start_dma_handler() {
  if (dma_handler_running) {
    return;
  }
  dma_handler_running = true;
  std::thread(&SocketClient::dma_read_handler_thread, this).detach();
  std::thread(&SocketClient::dma_write_handler_thread, this).detach();
}

void SocketClient::dma_read_handler_thread() {
  while (dma_handler_running && socket_initialized && dma_read_sock_fd >= 0) {
    // Receive DMA read request
    dma_read_req_t dma_read_req;
    if (!recv_dma_read_request(dma_read_req)) {
      break;
    }

    // Handle DMA read
    dma_data_128_t read_data =
        handle_dma_read(dma_read_req.addr, dma_read_req.size);

    // Send DMA read response
    dma_read_resp_t dma_read_resp;
    dma_read_resp.header.msg_type = MSG_TYPE_DMA_READ_RESP;
    dma_read_resp.header.reserved = 0;
    dma_read_resp.data_lo = read_data.lo;
    dma_read_resp.data_hi = read_data.hi;

    if (!send_dma_read_response(dma_read_resp)) {
      break;
    }
  }
}

void SocketClient::dma_write_handler_thread() {
  while (dma_handler_running && socket_initialized && dma_write_sock_fd >= 0) {
    // Receive DMA write request
    dma_write_req_t dma_write_req;
    if (!recv_dma_write_request(dma_write_req)) {
      break;
    }

    // Handle DMA write
    dma_data_128_t write_data;
    write_data.lo = dma_write_req.data_lo;
    write_data.hi = dma_write_req.data_hi;
    handle_dma_write(dma_write_req.addr, write_data, dma_write_req.size);

    // Send DMA write response
    dma_write_resp_t dma_write_resp;
    dma_write_resp.header.msg_type = MSG_TYPE_DMA_WRITE_RESP;
    dma_write_resp.header.reserved = 0;
    dma_write_resp.reserved = 0;

    if (!send_dma_write_response(dma_write_resp)) {
      break;
    }
  }
}
