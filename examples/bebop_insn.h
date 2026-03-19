/* Bebop custom-0 (RoCC opcode 0x0b) encoding helper for RISC-V.
 * Encoding: [31:25]=funct7, [24:20]=rs2, [19:15]=rs1, [14]=xd, [13]=xs1, [12]=xs2, [11:7]=rd, [6:0]=0x0b.
 * We use rd=a0(10), rs1=a1(11), rs2=a2(12), xd=xs1=xs2=1 so operands come from a1,a2 and result to a0.
 */
#ifndef BEBOP_INSN_H
#define BEBOP_INSN_H

#include <stdint.h>

/* BEMU funct codes (Buckyball) */
#define BEMU_MSET        23
#define BEMU_MVIN        24
#define BEMU_MVOUT       25
#define BEMU_MUL_WARP16  32
#define BEMU_TRANSPOSE   34

/* Execute BEMU custom0: xs1 in a1, xs2 in a2; result in a0. One encoding per funct (compile-time). */
static inline uint64_t bemu_custom0(uint32_t funct, uint64_t xs1, uint64_t xs2)
{
  register uint64_t a0 asm("a0");
  register uint64_t a1 asm("a1") = xs1;
  register uint64_t a2 asm("a2") = xs2;
  switch (funct) {
    case BEMU_MSET:
      /* rd=a0, rs1=a1, rs2=a2, xd=1, xs1=1, xs2=1, funct7=23 */
      asm volatile (".word 0x2ec5f50b" : "=r"(a0) : "r"(a1), "r"(a2) : "memory"); break;
    case BEMU_MVIN:
      /* funct7=24 */
      asm volatile (".word 0x30c5f50b" : "=r"(a0) : "r"(a1), "r"(a2) : "memory"); break;
    case BEMU_MVOUT:
      /* funct7=25 */
      asm volatile (".word 0x32c5f50b" : "=r"(a0) : "r"(a1), "r"(a2) : "memory"); break;
    case BEMU_MUL_WARP16:
      /* funct7=32 */
      asm volatile (".word 0x40c5f50b" : "=r"(a0) : "r"(a1), "r"(a2) : "memory"); break;
    case BEMU_TRANSPOSE:
      /* funct7=34 */
      asm volatile (".word 0x44c5f50b" : "=r"(a0) : "r"(a1), "r"(a2) : "memory"); break;
    default:
      asm volatile (".word 0x0b" : "=r"(a0) : "r"(a1), "r"(a2) : "memory"); /* unknown -> BEMU returns 0 or error */ break;
  }
  return a0;
}

#endif
