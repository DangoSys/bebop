#include "comp.h"
#include <string.h>
#include <stdint.h>
#include <riscv_vector.h>

void spmm(const csr_t *A, const double *B, int N, double *C) {
  const int M = A->rows;
  memset(C, 0, (size_t)M * N * sizeof(double));
  for (int i = 0; i < M; i++) {
    for (int p = A->row_ptr[i]; p < A->row_ptr[i + 1]; p++) {
      const int j = A->col_idx[p];
      const double a = A->val[p];
      for (int n = 0; n < N; n++)
        C[i * N + n] += a * B[j * N + n];
    }
  }
}


#include "inst.c"

void spmm_bb(const csr_t *A, const double *B, int N, double *C) {
  const int M = A->rows;
  memset(C, 0, (size_t)M * N * sizeof(double));

  /* bank 0 stores B's row, bank 1 stores C's row, depth=1, stride=row size in bytes. */
  const uint32_t bank_b = 0;
  const uint32_t bank_c = 1;
  const uint32_t depth_one = 1;
  const uint32_t stride_row = (uint32_t)((size_t)N * sizeof(double));

  for (int i = 0; i < M; i++) {
    for (int p = A->row_ptr[i]; p < A->row_ptr[i + 1]; p++) {
      const int j = A->col_idx[p];
      const double a = A->val[p];

      const double *rowB = B + (size_t)j * N;
      double *rowC = C + (size_t)i * N;

      /* 1) Memory sequence: whole B[j,:] as a block, use MVIN to pull into NPU buffer. */
      bb_mvin(rowB, bank_b, depth_one, stride_row);

      /* Encode j (col of A / row of B), i (row of C), N (width) into op1/op2/op3 (8-bit each). */
      const uint32_t op1 = (uint32_t)j;
      const uint32_t op2 = (uint32_t)i;
      const uint32_t op3 = (uint32_t)N;
      bb_gemm(op1, op2, op3);

      /* 3) MVOUT: write back C[i,:] row, drive one write access; specific data is determined by current NPU implementation. */
      bb_mvout(rowC, bank_c, depth_one, stride_row);
    }
  }
}

void spmm_rvv(const csr_t *A, const double *B, int N, double *C) {
  const int M = A->rows;
  memset(C, 0, (size_t)M * N * sizeof(double));

  /* bank 0 stores B's row, bank 1 stores C's row, depth=1, stride=row size in bytes. */
  const uint32_t bank_b = 0;
  const uint32_t bank_c = 1;
  const uint32_t depth_one = 1;
  const uint32_t stride_row = (uint32_t)((size_t)N * sizeof(double));

  for (int i = 0; i < M; i++) {
    /* 用 RVV 算这一行的基地址 rowC（虽等价于标量，但可触发 RVV 指令生成）。 */
    uintptr_t baseC = (uintptr_t)C;
    uint64_t offset_bytes = (uint64_t)i * (uint64_t)N * (uint64_t)sizeof(double);
    size_t vl_row = vsetvl_e64m1(1);
    vuint64m1_t vBase = vmv_v_x_u64m1(baseC, vl_row);
    vuint64m1_t vOff  = vmv_v_x_u64m1(offset_bytes, vl_row);
    vuint64m1_t vAddr = vadd_vv_u64m1(vBase, vOff, vl_row);
    uintptr_t addr_row = 0;
    vse64_v_u64m1(&addr_row, vAddr, vl_row);
    double *rowC = (double *)addr_row;

    for (int p = A->row_ptr[i]; p < A->row_ptr[i + 1]; p++) {
      const int j = A->col_idx[p];
      const double a = A->val[p];

      const double *rowB = B + (size_t)j * N;

      /* 1) Memory sequence: whole B[j,:] as a block, use MVIN to pull into NPU buffer. */
      bb_mvin(rowB, bank_b, depth_one, stride_row);

      /* Encode j (col of A / row of B), i (row of C), N (width) into op1/op2/op3 (8-bit each). */
      const uint32_t op1 = (uint32_t)j;
      const uint32_t op2 = (uint32_t)i;
      const uint32_t op3 = (uint32_t)N;
      bb_gemm(op1, op2, op3);

      /* 3) MVOUT: write back C[i,:] row, drive one write access; specific data is determined by current NPU implementation. */
      bb_mvout(rowC, bank_c, depth_one, stride_row);
    }
  }
}
