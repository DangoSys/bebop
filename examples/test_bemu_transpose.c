/*
 * Test TRANSPOSE: load 16x16 matrix via MVIN, transpose to another bank, MVOUT.
 */
#include <stdio.h>
#include <stdlib.h>
#include "bebop_insn.h"

#define CHECK(cond, msg) do { if (!(cond)) { fprintf(stderr, "FAIL: %s\n", msg); exit(1); } } while (0)

#define N        16
#define MAT_SZ   (N * N * sizeof(uint64_t))
#define N_BLOCKS (MAT_SZ / 16)

static uint64_t mat_src[N][N] __attribute__((aligned(16)));
static uint64_t mat_dst[N][N] __attribute__((aligned(16)));

static uint64_t make_mvin_xs1(unsigned bank_id, uintptr_t mem_addr)
{
  return (bank_id & 0x1F) | (((uint64_t)(uint32_t)mem_addr) << 27);
}

static uint64_t make_mvin_xs2(unsigned depth, unsigned stride)
{
  return (depth & 0x3FF) | ((stride & 0x7FFFF) << 10);
}

int main(void)
{
  printf("BEMU TRANSPOSE test\n");

  for (int i = 0; i < N; i++)
    for (int j = 0; j < N; j++) {
      mat_src[i][j] = (uint64_t)(i * N + j);
      mat_dst[i][j] = 0;
    }

  uint64_t xs1 = 0;
  uint64_t xs2 = N | (N << 5) | (1 << 10);
  uint64_t res = bemu_custom0(BEMU_MSET, xs1, xs2);
  CHECK(res == BEMU_MSET, "MSET bank0");
  xs1 = 1; xs2 = N | (N << 5) | (1 << 10);
  res = bemu_custom0(BEMU_MSET, xs1, xs2);
  CHECK(res == BEMU_MSET, "MSET bank1");

  xs1 = make_mvin_xs1(0, (uintptr_t)mat_src);
  xs2 = make_mvin_xs2(N_BLOCKS, 1);
  res = bemu_custom0(BEMU_MVIN, xs1, xs2);
  CHECK(res == BEMU_MVIN, "MVIN -> bank0");

  xs1 = 0 | (1 << 10);
  xs2 = 16;
  res = bemu_custom0(BEMU_TRANSPOSE, xs1, xs2);
  CHECK(res == BEMU_TRANSPOSE, "TRANSPOSE");

  xs1 = make_mvin_xs1(1, (uintptr_t)mat_dst);
  xs2 = make_mvin_xs2(N_BLOCKS, 1);
  res = bemu_custom0(BEMU_MVOUT, xs1, xs2);
  CHECK(res == BEMU_MVOUT, "MVOUT bank1 -> mat_dst");

  for (int i = 0; i < N; i++)
    for (int j = 0; j < N; j++)
      CHECK(mat_dst[i][j] == (uint64_t)(j * N + i), "TRANSPOSE result mismatch");

  printf("TRANSPOSE OK\n");
  return 0;
}
