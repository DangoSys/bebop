/*
 * Integration test: MSET, MVIN, MUL_WARP16, MVOUT, TRANSPOSE, MVOUT.
 * Verifies full pipeline and memory sync between Spike and BEMU.
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
static uint64_t mat_ct[N][N] __attribute__((aligned(16)));

static uint64_t make_mvin_xs1(unsigned bank_id, uintptr_t mem_addr) {
  return (bank_id & 0x1F) | (((uint64_t)(uint32_t)mem_addr) << 27);
}

static uint64_t make_mvin_xs2(unsigned depth, unsigned stride) {
  return (depth & 0x3FF) | ((stride & 0x7FFFF) << 10);
}

int main(void) {
  printf("BEMU integration test (full pipeline)\n");

  for (int i = 0; i < N; i++)
    for (int j = 0; j < N; j++) {
      mat_a[i][j] = (i == j) ? 1 : 0;
      mat_b[i][j] = (uint64_t)(i * N + j);
      mat_c[i][j] = 0;
      mat_ct[i][j] = 0;
    }

  uint64_t xs1, xs2, res;

  /* Alloc banks 0,1,2,3 */
  xs2 = N | (N << 5) | (1 << 10);
  for (int b = 0; b < 4; b++) {
    res = bemu_custom0(BEMU_MSET, (uint64_t)b, xs2);
    CHECK(res == BEMU_MSET, "MSET");
  }

  /* MVIN A -> bank0, B -> bank1 */
  xs1 = make_mvin_xs1(0, (uintptr_t)mat_a);
  xs2 = make_mvin_xs2(N_BLOCKS, 1);
  res = bemu_custom0(BEMU_MVIN, xs1, xs2);
  CHECK(res == BEMU_MVIN, "MVIN A");
  xs1 = make_mvin_xs1(1, (uintptr_t)mat_b);
  res = bemu_custom0(BEMU_MVIN, xs1, xs2);
  CHECK(res == BEMU_MVIN, "MVIN B");

  /* C = A * B (I * B = B) -> bank2 */
  xs1 = 0 | (1 << 5) | (2 << 10);
  xs2 = 16;
  res = bemu_custom0(BEMU_MUL_WARP16, xs1, xs2);
  CHECK(res == BEMU_MUL_WARP16, "MUL_WARP16");

  /* MVOUT bank2 -> mat_c */
  xs1 = make_mvin_xs1(2, (uintptr_t)mat_c);
  xs2 = make_mvin_xs2(N_BLOCKS, 1);
  res = bemu_custom0(BEMU_MVOUT, xs1, xs2);
  CHECK(res == BEMU_MVOUT, "MVOUT C");

  for (int i = 0; i < N; i++)
    for (int j = 0; j < N; j++)
      CHECK(mat_c[i][j] == mat_b[i][j], "C != B after I*B");

  /* TRANSPOSE bank2 -> bank3 */
  xs1 = 2 | (3 << 10);
  xs2 = 16;
  res = bemu_custom0(BEMU_TRANSPOSE, xs1, xs2);
  CHECK(res == BEMU_TRANSPOSE, "TRANSPOSE");

  /* MVOUT bank3 -> mat_ct */
  xs1 = make_mvin_xs1(3, (uintptr_t)mat_ct);
  xs2 = make_mvin_xs2(N_BLOCKS, 1);
  res = bemu_custom0(BEMU_MVOUT, xs1, xs2);
  CHECK(res == BEMU_MVOUT, "MVOUT Ct");

  for (int i = 0; i < N; i++)
    for (int j = 0; j < N; j++)
      CHECK(mat_ct[i][j] == mat_b[j][i], "Ct != B^T");

  printf("Integration test passed (MSET+MVIN+MUL_WARP16+MVOUT+TRANSPOSE+MVOUT).\n");
  return 0;
}
