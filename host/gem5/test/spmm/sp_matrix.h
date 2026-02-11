/*
 * Sparse matrix (CSR) and generators for large matrices.
 */

#ifndef SP_MATRIX_H
#define SP_MATRIX_H

#include <stddef.h>

/* CSR: row_ptr + col_idx + val (variable nnz per row). */
typedef struct {
  int rows, cols, nnz;
  double *val;
  int *col_idx;
  int *row_ptr;
} csr_t;

void csr_free(csr_t *c);

/* Row-wise sparse: 随机裁行，剩 num_rows 行. row_idx=哪些行, val=稠密 num_rows×cols. */
typedef struct {
  int rows, cols;
  int num_rows;
  int *row_idx;      /* which rows kept, length num_rows */
  double *val;       /* dense block num_rows×cols, row-major */
} row_sparse_t;

void row_sparse_free(row_sparse_t *r);

/* --- CSR-style: random (row,col) over whole matrix --- */
csr_t *csr_random(int M, int N, int nnz);
csr_t *csr_random_density(int M, int N, double density);
csr_t *csr_random_density_seed(int M, int N, double density, unsigned seed);

/* --- Row-wise sparse: num_rows rows have data, each row is full (cols elements). --- */
row_sparse_t *row_sparse_random(int M, int N, int num_rows);

/* Convert row_sparse_t to csr_t for use with spmm. */
csr_t *csr_from_row_sparse(const row_sparse_t *r);

#endif
