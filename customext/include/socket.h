#ifndef _SOCKET_H
#define _SOCKET_H

#include <cstdint>

// Socket configuration
#define SOCKET_PORT 9999
#define SOCKET_HOST "127.0.0.1"

// Message structures for socket communication
struct socket_msg_t {
  uint32_t funct;
  uint64_t xs1;
  uint64_t xs2;
};

struct socket_resp_t {
  uint64_t result;
};

// Socket client class
class SocketClient {
public:
  SocketClient();
  ~SocketClient();
  
  // Initialize and connect to socket server
  bool init();
  
  // Close socket connection
  void close();
  
  // Send request and wait for response
  uint64_t send_and_wait(uint32_t funct, uint64_t xs1, uint64_t xs2);
  
  // Check if socket is connected
  bool is_connected() const { return socket_initialized; }

private:
  int sock_fd;
  bool socket_initialized;
  
  bool send_request(const socket_msg_t& msg);
  bool recv_response(socket_resp_t& resp);
};

#endif // _SOCKET_H

