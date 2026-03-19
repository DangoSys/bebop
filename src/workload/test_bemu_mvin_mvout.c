/*
 * Test MVIN/MVOUT with Spike-BEMU memory sync.
 * Writes data to a buffer, MVIN to bank, MVOUT to another buffer, then compares.
 */
#include "bebop_insn.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CHECK(cond, msg)                                                                           \
  do {                                                                                             \
    if (!(cond)) {                                                                                 \
      fprintf(stderr, "FAIL: %s\n", msg);                                                          \
      exit(1);                                                                                     \
    }                                                                                              \
  } while (0)

#define N_BLOCKS 4
#define BLOCK_SZ 16
#define BUF_SZ (N_BLOCKS * BLOCK_SZ)

static uint8_t src_buf[BUF_SZ] __attribute__((aligned(16)));
static uint8_t dst_buf[BUF_SZ] __attribute__((aligned(16)));

static uint64_t make_mvin_xs1(unsigned bank_id, uintptr_t mem_addr) {
  return (bank_id & 0x1F) | (((uint64_t)(uint32_t)mem_addr) << 27);
}

static uint64_t make_mvin_xs2(unsigned depth, unsigned stride) {
  return (depth & 0x3FF) | ((stride & 0x7FFFF) << 10);
}

int main(void) {
  printf("BEMU MVIN/MVOUT test (memory sync)\n");

  for (size_t i = 0; i < BUF_SZ; i++)
    src_buf[i] = (uint8_t)(0x40 + (i & 0x3F));
  memset(dst_buf, 0, BUF_SZ);

  uint64_t xs1 = 0;
  uint64_t xs2 = 4 | (4 << 5) | (1 << 10);
  uint64_t res = bemu_custom0(BEMU_MSET, xs1, xs2);
  CHECK(res == BEMU_MSET, "MSET bank0");

  uint64_t mvin_xs1 = make_mvin_xs1(0, (uintptr_t)src_buf);
  uint64_t mvin_xs2 = make_mvin_xs2(N_BLOCKS, 1);
  res = bemu_custom0(BEMU_MVIN, mvin_xs1, mvin_xs2);
  CHECK(res == BEMU_MVIN, "MVIN");

  uint64_t mvout_xs1 = make_mvin_xs1(0, (uintptr_t)dst_buf);
  res = bemu_custom0(BEMU_MVOUT, mvout_xs1, mvin_xs2);
  CHECK(res == BEMU_MVOUT, "MVOUT");

  int cmp = memcmp(src_buf, dst_buf, BUF_SZ);
  if (cmp != 0) {
    fprintf(stderr, "FAIL: MVIN/MVOUT round-trip data mismatch\n");
    fprintf(stderr,
            "  src_buf=%p dst_buf=%p (if both > 512KB, check spike extension build/config)\n",
            (void *)src_buf, (void *)dst_buf);
    fprintf(stderr, "  first byte: src=0x%02x dst=0x%02x\n", src_buf[0], dst_buf[0]);
    exit(1);
  }

  printf("MVIN/MVOUT round-trip OK (%u blocks)\n", N_BLOCKS);
  return 0;
}
