/*
 * Sparse matrix (CSR) generators. Supports large M,N by only allocating nnz entries.
 */

#include "sp_matrix.h"
#include <stdlib.h>
#include <string.h>
#include <stdio.h>

void csr_free(csr_t *c) {
  if (!c) return;
  free(c->val);
  free(c->col_idx);
  free(c->row_ptr);
  c->val = NULL;
  c->col_idx = NULL;
  c->row_ptr = NULL;
}

typedef struct { int row; int col; double val; } coord_t;

static int cmp_coord(const void *a, const void *b) {
  const coord_t *p = (const coord_t *)a;
  const coord_t *q = (const coord_t *)b;
  if (p->row != q->row) return (p->row > q->row) - (p->row < q->row);
  return (p->col > q->col) - (p->col < q->col);
}

/* Build CSR from sorted coord array; merge duplicate (row,col) by summing val. */
static csr_t *csr_from_sorted_coords(int M, int N, coord_t *coord, int nnz_in) {
  if (nnz_in <= 0) return NULL;
  int nnz = 0;
  for (int k = 0; k < nnz_in; k++) {
    if (nnz > 0 && coord[nnz - 1].row == coord[k].row && coord[nnz - 1].col == coord[k].col)
      coord[nnz - 1].val += coord[k].val;
    else {
      if (nnz < k) coord[nnz] = coord[k];
      nnz++;
    }
  }
  csr_t *A = calloc(1, sizeof(csr_t));
  if (!A) return NULL;
  A->rows = M;
  A->cols = N;
  A->nnz = nnz;
  A->val = malloc((size_t)nnz * sizeof(double));
  A->col_idx = malloc((size_t)nnz * sizeof(int));
  A->row_ptr = malloc((size_t)(M + 1) * sizeof(int));
  if (!A->val || !A->col_idx || !A->row_ptr) {
    csr_free(A);
    free(A);
    return NULL;
  }
  for (int k = 0; k < nnz; k++) {
    A->val[k] = coord[k].val;
    A->col_idx[k] = coord[k].col;
  }
  int *row_ptr = A->row_ptr;
  row_ptr[0] = 0;
  for (int i = 0, k = 0; i < M; i++) {
    while (k < nnz && coord[k].row == i) k++;
    row_ptr[i + 1] = k;
  }
  return A;
}

csr_t *csr_random(int M, int N, int nnz_req) {
  if (M <= 0 || N <= 0 || nnz_req <= 0) return NULL;
  if ((size_t)nnz_req > (size_t)M * (size_t)N) nnz_req = M * N;
  coord_t *coord = malloc((size_t)nnz_req * sizeof(coord_t));
  if (!coord) return NULL;
  for (int k = 0; k < nnz_req; k++) {
    coord[k].row = rand() % M;
    coord[k].col = rand() % N;
    coord[k].val = (double)(rand() % 1000) / 1000.0;
  }
  qsort(coord, (size_t)nnz_req, sizeof(coord_t), cmp_coord);
  csr_t *A = csr_from_sorted_coords(M, N, coord, nnz_req);
  free(coord);
  return A;
}

csr_t *csr_random_density(int M, int N, double density) {
  if (M <= 0 || N <= 0 || density <= 0.0 || density > 1.0) return NULL;
  size_t total = (size_t)M * (size_t)N;
  size_t nnz = (size_t)((double)total * density);
  if (nnz == 0) nnz = 1;
  if (nnz > total) nnz = total;
  return csr_random(M, N, (int)nnz);
}

csr_t *csr_random_density_seed(int M, int N, double density, unsigned seed) {
  srand(seed);
  return csr_random_density(M, N, density);
}

void row_sparse_free(row_sparse_t *r) {
  if (!r) return;
  free(r->row_idx);
  free(r->val);
  r->row_idx = NULL;
  r->val = NULL;
}

/* Pick k distinct in [0, N), write to buf[0..k-1]. */
static void pick_distinct(int N, int k, int *buf) {
  if (k >= N) {
    for (int i = 0; i < N; i++) buf[i] = i;
    return;
  }
  for (int i = 0; i < k; i++) {
    for (;;) {
      int c = rand() % N;
      int j;
      for (j = 0; j < i && buf[j] != c; j++) {}
      if (j == i) { buf[i] = c; break; }
    }
  }
}

static int cmp_int(const void *a, const void *b) {
  int x = *(const int *)a, y = *(const int *)b;
  return (x > y) - (x < y);
}

/* Randomly pick num_rows rows from M, dense block num_rows*cols. */
row_sparse_t *row_sparse_random(int M, int N, int num_rows) {
  if (M <= 0 || N <= 0 || num_rows <= 0) return NULL;
  if (num_rows > M) num_rows = M;
  row_sparse_t *r = calloc(1, sizeof(row_sparse_t));
  if (!r) return NULL;
  r->rows = M;
  r->cols = N;
  r->num_rows = num_rows;
  r->row_idx = malloc((size_t)num_rows * sizeof(int));
  r->val = malloc((size_t)num_rows * (size_t)N * sizeof(double));
  if (!r->row_idx || !r->val) {
    row_sparse_free(r);
    free(r);
    return NULL;
  }
  pick_distinct(M, num_rows, r->row_idx);  /* which rows to keep (random) */
  qsort(r->row_idx, (size_t)num_rows, sizeof(int), cmp_int);  /* store in row order */
  for (int i = 0; i < num_rows; i++)
    for (int j = 0; j < N; j++)
      r->val[i * N + j] = (double)(rand() % 1000) / 1000.0;
  return r;
}

csr_t *csr_from_row_sparse(const row_sparse_t *r) {
  if (!r || !r->val || !r->row_idx) return NULL;
  const int M = r->rows, N = r->cols, nr = r->num_rows;
  /* nnz = nr * N (each stored row is full) */
  const int nnz = nr * N;
  csr_t *A = calloc(1, sizeof(csr_t));
  if (!A) return NULL;
  A->rows = M;
  A->cols = N;
  A->nnz = nnz;
  A->val = malloc((size_t)nnz * sizeof(double));
  A->col_idx = malloc((size_t)nnz * sizeof(int));
  A->row_ptr = malloc((size_t)(M + 1) * sizeof(int));
  if (!A->val || !A->col_idx || !A->row_ptr) {
    csr_free(A);
    free(A);
    return NULL;
  }
  for (int i = 0; i < M + 1; i++)
    A->row_ptr[i] = 0;
  for (int i = 0; i < nr; i++)
    A->row_ptr[r->row_idx[i] + 1] = N;
  for (int i = 0; i < M; i++)
    A->row_ptr[i + 1] += A->row_ptr[i];
  for (int i = 0; i < nr; i++) {
    int orig_row = r->row_idx[i];
    int start = A->row_ptr[orig_row];
    for (int j = 0; j < N; j++) {
      A->col_idx[start + j] = j;
      A->val[start + j] = r->val[i * N + j];
    }
  }
  return A;
}
