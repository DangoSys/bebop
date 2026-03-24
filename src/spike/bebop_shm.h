/* Shared layout for Spike bebop_rocc and Rust BEMU worker (must stay in sync). */
#ifndef BEBOP_SHM_H
#define BEBOP_SHM_H

#include <stdint.h>

#define BEBOP_SHM_SIZE 4096

#define BEBOP_OP_CMD_REQ 1u
#define BEBOP_OP_CMD_RESP 2u
#define BEBOP_OP_MEM_REQ 3u
#define BEBOP_OP_MEM_RESP 4u

#define BEBOP_CMD_HANDLE 2u
#define BEBOP_CMD_SHUTDOWN 255u

#define BEBOP_MEM_WRITE 1u
#define BEBOP_MEM_READ 2u

typedef struct {
  uint32_t op;
  uint32_t sender_id;
  uint32_t receiver_id;
  uint32_t cmd_code;
  uint32_t mem_rw;
  uint32_t funct;
  uint32_t size;
  int32_t err;
  uint32_t _pad0;
  uint64_t msg_id;
  uint64_t txn_id;
  uint64_t xs1;
  uint64_t xs2;
  uint64_t result;
  uint64_t addr;
  uint8_t data[16];
  uint32_t sync_flags;
  uint32_t line_blocks;
  uint32_t depth;
  uint32_t _pad1;
  uint64_t mem_addr;
  uint64_t stride;
} bebop_msg_t;

typedef struct {
  uint64_t req;
  uint64_t ack;
  bebop_msg_t msg;
} bebop_lane_t;

typedef struct {
  bebop_lane_t cmd;
  bebop_lane_t mem;
} bebop_shm_t;

#endif
