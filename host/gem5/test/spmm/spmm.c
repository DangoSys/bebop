/*
 * SpMM example: 256×1024 sparse × 1024×128 dense.
 */

#include "sp_matrix.h"
#include "comp.h"
#include <stdio.h>
#include <stdlib.h>

int main(void) {
  const int M = 256, K = 1024, N = 128;
  srand(42);
  csr_t *A = csr_random(M, K, 16 * 1024);  /* 256×1024, 16k nnz */
  if (!A) {
    fprintf(stderr, "failed to create CSR\n");
    return 1;
  }

  double *B = malloc((size_t)K * N * sizeof(double));
  double *C = calloc((size_t)M * N, sizeof(double));
  if (!B || !C) {
    csr_free(A);
    free(A);
    free(B);
    free(C);
    return 1;
  }
  for (int i = 0; i < K * N; i++)
    B[i] = (double)(rand() % 1000) / 1000.0;

  spmm(A, B, N, C);

  printf("SpMM C = A*B (A CSR %dx%d nnz=%d, B dense %dx%d):\n", M, K, A->nnz, K, N);
  printf("C[0][0..7] =");
  for (int n = 0; n < 8; n++)
    printf(" %g", C[n]);
  printf("\n");

  free(C);
  free(B);
  csr_free(A);
  free(A);
  return 0;
}
