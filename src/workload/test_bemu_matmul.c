/*
 * Test MUL_WARP16: load two 16x16 matrices via MVIN, multiply, MVOUT result.
 * Uses identity-like and 2*identity to get predictable result.
 */
#include "bebop_insn.h"
#include <stdio.h>
#include <stdlib.h>

#define CHECK(cond, msg)                                                                           \
  do {                                                                                             \
    if (!(cond)) {                                                                                 \
      fprintf(stderr, "FAIL: %s\n", msg);                                                          \
      exit(1);                                                                                     \
    }                                                                                              \
  } while (0)

#define N 16
#define MAT_SZ (N * N * sizeof(uint64_t))
#define N_BLOCKS (MAT_SZ / 16)

static uint64_t mat_a[N][N] __attribute__((aligned(16)));
static uint64_t mat_b[N][N] __attribute__((aligned(16)));
static uint64_t mat_c[N][N] __attribute__((aligned(16)));

static uint64_t make_mvin_xs1(unsigned bank_id, uintptr_t mem_addr) {
  return (bank_id & 0x1F) | (((uint64_t)(uint32_t)mem_addr) << 27);
}

static uint64_t make_mvin_xs2(unsigned depth, unsigned stride) {
  return (depth & 0x3FF) | ((stride & 0x7FFFF) << 10);
}

int main(void) {
  printf("BEMU MUL_WARP16 test\n");

  for (int i = 0; i < N; i++)
    for (int j = 0; j < N; j++) {
      mat_a[i][j] = (i == j) ? 1 : 0;
      mat_b[i][j] = (i == j) ? 2 : 0;
      mat_c[i][j] = 0;
    }

  uint64_t xs1, xs2, res;

  xs1 = 0;
  xs2 = N | (N << 5) | (1 << 10);
  res = bemu_custom0(BEMU_MSET, xs1, xs2);
  CHECK(res == BEMU_MSET, "MSET bank0");
  xs1 = 1;
  xs2 = N | (N << 5) | (1 << 10);
  res = bemu_custom0(BEMU_MSET, xs1, xs2);
  CHECK(res == BEMU_MSET, "MSET bank1");
  xs1 = 2;
  xs2 = N | (N << 5) | (1 << 10);
  res = bemu_custom0(BEMU_MSET, xs1, xs2);
  CHECK(res == BEMU_MSET, "MSET bank2");

  xs1 = make_mvin_xs1(0, (uintptr_t)mat_a);
  xs2 = make_mvin_xs2(N_BLOCKS, 1);
  res = bemu_custom0(BEMU_MVIN, xs1, xs2);
  CHECK(res == BEMU_MVIN, "MVIN mat_a -> bank0");

  xs1 = make_mvin_xs1(1, (uintptr_t)mat_b);
  res = bemu_custom0(BEMU_MVIN, xs1, xs2);
  CHECK(res == BEMU_MVIN, "MVIN mat_b -> bank1");

  xs1 = 0 | (1 << 5) | (2 << 10);
  xs2 = 16;
  res = bemu_custom0(BEMU_MUL_WARP16, xs1, xs2);
  CHECK(res == BEMU_MUL_WARP16, "MUL_WARP16");

  xs1 = make_mvin_xs1(2, (uintptr_t)mat_c);
  xs2 = make_mvin_xs2(N_BLOCKS, 1);
  res = bemu_custom0(BEMU_MVOUT, xs1, xs2);
  CHECK(res == BEMU_MVOUT, "MVOUT bank2 -> mat_c");

  for (int i = 0; i < N; i++)
    for (int j = 0; j < N; j++) {
      uint64_t expect = (i == j) ? 2 : 0;
      CHECK(mat_c[i][j] == expect, "MUL_WARP16 result mismatch");
    }

  printf("MUL_WARP16 I*2I=2I OK\n");
  return 0;
}
