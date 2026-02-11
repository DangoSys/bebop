/*
 * Buckyball NPU custom instruction encoding (RISC-V custom-3).
 * For SpMM: mvin / mvout / compute config.
 */

#ifndef BUCKYBALL_INST_H
#define BUCKYBALL_INST_H

#include <stdint.h>

#define STR(x)  STR_(x)
#define STR_(x) #x

/* custom-3 opcode (RISC-V) */
#define CUSTOM_3 0x7b

/* Field encoding macro with start and end bit */
#define FIELD(val, start_bit, end_bit)                                         \
  (((val) & ((1UL << ((end_bit) - (start_bit) + 1)) - 1)) << (start_bit))

/* Generic RISC-V custom instruction macro (R-type, rd=x0) */
#define BUCKYBALL_INSTRUCTION_R_R(rs1_val, rs2_val, func7)                     \
  asm volatile(".insn r " STR(CUSTOM_3) ", 0x3, %c2, x0, %0, %1"               \
               :                                                               \
               : "r"(rs1_val), "r"(rs2_val), "i"(func7)                        \
               : "memory")

/* --- MVIN: move data from memory into NPU buffer ---
 * rs1: mem_addr[31:0]
 * rs2: bank_id[4:0] | depth[9:0]@5 | stride[18:0]@15
 */
#define BB_MVIN_FUNC7 24
#define bb_mvin(mem_addr, bank_id, depth, stride)                              \
  BUCKYBALL_INSTRUCTION_R_R(                                                   \
      FIELD((uintptr_t)(mem_addr), 0, 31),                                     \
      (FIELD((bank_id), 0, 4) | FIELD((depth), 5, 14) | FIELD((stride), 15, 33)), \
      BB_MVIN_FUNC7)

/* --- MVOUT: move data from NPU buffer to memory --- */
#define BB_MVOUT_FUNC7 25
#define bb_mvout(mem_addr, bank_id, depth, stride)                             \
  BUCKYBALL_INSTRUCTION_R_R(                                                   \
      FIELD((uintptr_t)(mem_addr), 0, 31),                                     \
      (FIELD((bank_id), 0, 4) | FIELD((depth), 5, 14) | FIELD((stride), 15, 33)), \
      BB_MVOUT_FUNC7)

/* --- VEC_MVIN (gather): base + 8 byte-offsets, each load = one vector. RV64. ---
 * rs1: base_addr[31:0] | vlen[32:40] (9 bits). rs2: o0[7:0]|o1[15:8]|...|o7[63:56].
 * bank_id in func7 (26..57).
 */
#define BB_MGATHER_FUNC7 26
#define bb_mgather(base_addr, vlen, bank_id, o0, o1, o2, o3, o4, o5, o6, o7)   \
  BUCKYBALL_INSTRUCTION_R_R(                                                   \
      (FIELD((uintptr_t)(base_addr), 0, 31) | FIELD((vlen), 32, 40) | FIELD((bank_id), 41, 45)), \
      (FIELD((o0) & 0xFF, 0, 7) | FIELD((o1) & 0xFF, 8, 15) |                  \
       FIELD((o2) & 0xFF, 16, 23) | FIELD((o3) & 0xFF, 24, 31) |               \
       FIELD((o4) & 0xFF, 32, 39) | FIELD((o5) & 0xFF, 40, 47) |               \
       FIELD((o6) & 0xFF, 48, 55) | FIELD((o7) & 0xFF, 56, 63)),               \
      BB_MGATHER_FUNC7)

/* --- GEMM: dense matrix multiply C = A*B (or C += A*B) ---
 * A: M×K, B: K×N, C: M×N. rs1: M[15:0] | K[15:0]@16, rs2: N[15:0]
 */
#define BB_GEMM_FUNC7 27
#define bb_gemm(op1_addr, op2_addr, op3_addr)               \
  BUCKYBALL_INSTRUCTION_R_R(                                \
      (FIELD((op1_addr), 0, 7) | FIELD((op2_addr), 8, 15)), \
      (FIELD((op3_addr), 0, 7)), \
      BB_GEMM_FUNC7)


// #define BB_DECODE_FUNC7 28

#define BB_DECODE_FINISH_FUNC7 29
#define bb_is_decode_finished() \
  BUCKYBALL_INSTRUCTION_R_R(0, 0, BB_DECODE_FINISH_FUNC7) \


#endif /* BUCKYBALL_INST_H */
