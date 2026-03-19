/*
 * Test program: run on Spike with BEMU extension.
 * Executes Bebop custom instructions (custom-0) and checks results.
 * Build: see examples/Makefile. Run: spike --extension=bebop_rocc pk test_bemu_custom
 * Set LD_LIBRARY_PATH to include directory containing libbemu.so.
 */
#include <stdio.h>
#include <stdlib.h>
#include "bebop_insn.h"

#define CHECK(cond, msg) do { if (!(cond)) { fprintf(stderr, "FAIL: %s\n", msg); exit(1); } } while (0)

int main(void)
{
  printf("BEMU custom instruction test (Spike + bebop_rocc)\n");

  /* MSET: alloc bank0, 4x4, alloc=1. xs1 = bank_id (0), xs2 = row | (col<<5) | (alloc<<10) */
  uint64_t xs1 = 0;                    /* bank_id 0 */
  uint64_t xs2 = 4 | (4 << 5) | (1 << 10);  /* row=4, col=4, alloc=1 */
  uint64_t res = bemu_custom0(BEMU_MSET, xs1, xs2);
  printf("MSET(0, 4, 4, alloc=1) => 0x%lx\n", (unsigned long)res);
  CHECK(res == BEMU_MSET, "MSET should return funct 23");

  /* MSET again on same bank: alloc=0 to release */
  xs2 = 4 | (4 << 5) | (0 << 10);
  res = bemu_custom0(BEMU_MSET, xs1, xs2);
  printf("MSET(0, release) => 0x%lx\n", (unsigned long)res);
  CHECK(res == BEMU_MSET, "MSET release should return 23");

  /* Alloc again for later tests */
  xs2 = 4 | (4 << 5) | (1 << 10);
  res = bemu_custom0(BEMU_MSET, xs1, xs2);
  CHECK(res == BEMU_MSET, "MSET re-alloc");

  printf("All BEMU custom instruction checks passed.\n");
  return 0;
}
