/*
 * SpMM: C = A * B, A CSR, B dense, C dense.
 */

#ifndef SPMM_COMP_H
#define SPMM_COMP_H

#include "sp_matrix.h"

/* C = A*B. A: M×K (CSR), B: K×N (dense), C: M×N (dense, row-major). C must be zeroed or will be overwritten. */
void spmm(const csr_t *A, const double *B, int N, double *C);

void spmm_bb(const csr_t *A, const double *B, int N, double *C);

void spmm_rvv(const csr_t *A, const double *B, int N, double *C);

#endif
