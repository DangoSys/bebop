// Drives `bebop_accel` (see bebop_accel.sv). Pulses clk after setting funct/xs1/xs2 so bank model
// updates.

#include <cstdint>
#include <cstdio>
#include <cstdlib>

#include "Vbebop_accel.h"
#include "verilated.h"

static VerilatedContext *g_ctx;
static Vbebop_accel *g_top;

static uint32_t g_digest_all_banks = 0;

extern "C" void bebop_cosim_init(void) {
  if (g_top) {
    return;
  }
  g_ctx = new VerilatedContext;
  g_top = new Vbebop_accel{g_ctx};
  g_top->clk = 0;
  g_top->digest_all_banks = 0;
  g_top->eval();
}

extern "C" void bebop_rust_mem_read16(uint64_t addr, uint64_t *lo, uint64_t *hi);
extern "C" void bebop_rust_mem_write16(uint64_t addr, uint64_t lo, uint64_t hi);

extern "C" void bebop_cosim_set_digest_all_banks(uint32_t v) { g_digest_all_banks = v ? 1u : 0u; }

extern "C" void dpi_mem_read16(uint64_t addr, uint64_t *lo, uint64_t *hi) {
  bebop_rust_mem_read16(addr, lo, hi);
}

extern "C" void dpi_mem_write16(uint64_t addr, uint64_t lo, uint64_t hi) {
  bebop_rust_mem_write16(addr, lo, hi);
}

extern "C" void bebop_cosim_issue(uint32_t funct, uint64_t xs1, uint64_t xs2) {
  if (!g_top || !g_ctx) {
    std::fprintf(stderr, "bebop_cosim_init was not called\n");
    std::abort();
  }
  g_top->digest_all_banks = g_digest_all_banks;
  g_top->funct = funct & 0x7f;
  g_top->xs1 = xs1;
  g_top->xs2 = xs2;
  g_top->clk = 0;
  g_top->eval();
  g_top->clk = 1;
  g_top->eval();
  g_top->clk = 0;
  g_top->eval();
  if ((funct & 0x7fU) == 64U) {
    const uint32_t f = funct & 0x7fU;
    const uint64_t iter = (xs1 >> 30);
    if (iter == 0 || (iter % 16) != 0) {
      std::fprintf(stderr, "bebop_cosim_issue: mul_warp16 bad iter=%llu\n",
                   static_cast<unsigned long long>(iter));
      std::abort();
    }
    const uint64_t extra_cycles = iter * 32ULL;
    for (uint64_t i = 0; i < extra_cycles; ++i) {
      g_top->funct = 0;
      g_top->xs1 = 0;
      g_top->xs2 = 0;
      g_top->clk = 0;
      g_top->eval();
      g_top->clk = 1;
      g_top->eval();
      g_top->clk = 0;
      g_top->eval();
    }
    g_top->funct = f;
    g_top->xs1 = xs1;
    g_top->xs2 = xs2;
    g_top->clk = 0;
    g_top->eval();
  }
}

extern "C" uint64_t bebop_cosim_read_result(void) {
  if (!g_top) {
    std::fprintf(stderr, "bebop_cosim_read_result: model is null\n");
    std::abort();
  }
  return static_cast<uint64_t>(g_top->result);
}

extern "C" uint64_t bebop_cosim_read_bank_digest_peek(void) {
  if (!g_top) {
    std::fprintf(stderr, "bebop_cosim_read_bank_digest_peek: model is null\n");
    std::abort();
  }
  return static_cast<uint64_t>(g_top->bank_digest_peek);
}

extern "C" void bebop_cosim_shutdown(void) {
  if (g_top) {
    delete g_top;
    g_top = nullptr;
  }
  if (g_ctx) {
    delete g_ctx;
    g_ctx = nullptr;
  }
}
