#include "bebop_insn.h"
#include <stdint.h>
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

#define DIM 16
#define KDIM 64
#define KTILE 16

_Static_assert(KDIM % KTILE == 0, "KDIM must be divisible by KTILE");
_Static_assert(KDIM % DIM == 0, "KDIM must be divisible by DIM");

static int8_t input_matrix_a[DIM * KDIM] __attribute__((aligned(64)));
static int8_t input_matrix_b[KDIM * DIM] __attribute__((aligned(64)));
static int32_t output_matrix[DIM * DIM] __attribute__((aligned(64)));
static int32_t expected_matrix[DIM * DIM] __attribute__((aligned(64)));
static int32_t zero_matrix[DIM * DIM] __attribute__((aligned(64))) = {0};

static uint64_t make_mvin_xs1(unsigned bank_id, uintptr_t mem_addr) {
  return (bank_id & 0xFF) | (((uint64_t)(uint32_t)mem_addr) << 27);
}

static uint64_t make_mvin_xs2(unsigned depth, unsigned stride) {
  return (depth & 0x3FF) | ((stride & 0x7FFFF) << 10);
}

static uint64_t make_mul_xs1(unsigned op1, unsigned op2, unsigned wr) {
  return (op1 & 0xFF) | ((uint64_t)(op2 & 0xFF) << 8) | ((uint64_t)(wr & 0xFF) << 16);
}

static uint64_t make_transpose_xs1(unsigned op1, unsigned wr) {
  return (op1 & 0xFF) | ((uint64_t)(wr & 0xFF) << 16);
}

static void clear_u8_matrix(int8_t *m, int rows, int cols) { memset(m, 0, (size_t)rows * cols); }
static void clear_u32_matrix(int32_t *m, int rows, int cols) {
  memset(m, 0, (size_t)rows * cols * sizeof(int32_t));
}

static int compare_u32_matrices(const int32_t *a, const int32_t *b, int rows, int cols) {
  for (int i = 0; i < rows * cols; i++) {
    if (a[i] != b[i]) {
      fprintf(stderr, "mismatch at %d: got %d expect %d\n", i, a[i], b[i]);
      return 0;
    }
  }
  return 1;
}

static void init_diag_ones(int8_t *a, int8_t *b, int32_t *expected) {
  clear_u8_matrix(a, DIM, KDIM);
  clear_u8_matrix(b, KDIM, DIM);
  clear_u32_matrix(expected, DIM, DIM);

  for (int k = 0; k < KDIM; k++) {
    int i = k % DIM;
    a[i * KDIM + k] = 1;
    b[k * DIM + i] = 1;
  }

  int diag_val = KDIM / DIM;
  for (int r = 0; r < DIM; r++) {
    expected[r * DIM + r] = diag_val;
  }
}

static void hw_matmul_tiled(int8_t *a, int8_t *b, int32_t *c) {
  uint32_t op1_bank_id = 0;
  uint32_t op2_bank_id = 1;
  uint32_t acc_bank_id = 2;
  uint32_t a_transposed_bank_id = 3;

  // vecunit layout: op banks cols=1, acc bank cols=4
  CHECK(bemu_custom0(BEMU_MSET, op1_bank_id, 1 | (1 << 5) | (1 << 10)) == BEMU_MSET, "MSET op1");
  CHECK(bemu_custom0(BEMU_MSET, op2_bank_id, 1 | (1 << 5) | (1 << 10)) == BEMU_MSET, "MSET op2");
  CHECK(bemu_custom0(BEMU_MSET, acc_bank_id, 1 | (4 << 5) | (1 << 10)) == BEMU_MSET, "MSET acc");
  CHECK(bemu_custom0(BEMU_MSET, a_transposed_bank_id, 1 | (1 << 5) | (1 << 10)) == BEMU_MSET,
        "MSET a_t");

  CHECK(bemu_custom0(BEMU_MVIN, make_mvin_xs1(acc_bank_id, (uintptr_t)zero_matrix),
                     make_mvin_xs2(DIM, 1)) == BEMU_MVIN,
        "MVIN zero acc");

  for (int k0 = 0; k0 < KDIM; k0 += KTILE) {
    // A is DIMxKDIM row-major; tile width is 16B, so row stride is KDIM/16 blocks.
    CHECK(bemu_custom0(BEMU_MVIN, make_mvin_xs1(op1_bank_id, (uintptr_t)(a + k0)),
                       make_mvin_xs2(KTILE, KDIM / 16)) == BEMU_MVIN,
          "MVIN A tile");
    CHECK(bemu_custom0(BEMU_MVIN, make_mvin_xs1(op2_bank_id, (uintptr_t)(b + k0 * DIM)),
                       make_mvin_xs2(KTILE, 1)) == BEMU_MVIN,
          "MVIN B tile");
    CHECK(bemu_custom0(BEMU_TRANSPOSE, make_transpose_xs1(op1_bank_id, a_transposed_bank_id),
                       KTILE) == BEMU_TRANSPOSE,
          "TRANSPOSE A tile");
    CHECK(bemu_custom0(BEMU_MUL_WARP16,
                       make_mul_xs1(a_transposed_bank_id, op2_bank_id, acc_bank_id),
                       KTILE) == BEMU_MUL_WARP16,
          "MUL_WARP16 tile");
  }

  CHECK(bemu_custom0(BEMU_MVOUT, make_mvin_xs1(acc_bank_id, (uintptr_t)c), make_mvin_xs2(DIM, 1)) ==
            BEMU_MVOUT,
        "MVOUT C");
}

int main(void) {
  printf("BEMU vecunit tiled matmul test\n");
  init_diag_ones(input_matrix_a, input_matrix_b, expected_matrix);
  clear_u32_matrix(output_matrix, DIM, DIM);

  hw_matmul_tiled(input_matrix_a, input_matrix_b, output_matrix);
  CHECK(compare_u32_matrices(output_matrix, expected_matrix, DIM, DIM),
        "vecunit tiled matmul mismatch");

  printf("vecunit tiled matmul PASSED\n");
  return 0;
}
