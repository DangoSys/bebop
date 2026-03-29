// Drives `bebop_accel` (see bebop_accel.sv). Combinational DUT: single eval().
// If you add clocked Chisel logic under arch/sims/bebop, extend this file to pulse clk
// N times before bebop_cosim_read_result(), and align vl_worker expectations.

#include <cstdint>
#include <cstdio>
#include <cstdlib>

#include "Vbebop_accel.h"
#include "verilated.h"

static VerilatedContext *g_ctx;
static Vbebop_accel *g_top;

extern "C" void bebop_cosim_init(void) {
  if (g_top) {
    return;
  }
  g_ctx = new VerilatedContext;
  g_top = new Vbebop_accel{g_ctx};
}

extern "C" void bebop_cosim_issue(uint32_t funct, uint64_t xs1, uint64_t xs2) {
  if (!g_top || !g_ctx) {
    std::fprintf(stderr, "bebop_cosim_init was not called\n");
    std::abort();
  }
  g_top->funct = funct & 0x7f;
  g_top->xs1 = xs1;
  g_top->xs2 = xs2;
  g_top->eval();
}

extern "C" uint64_t bebop_cosim_read_result(void) {
  if (!g_top) {
    std::fprintf(stderr, "bebop_cosim_read_result: model is null\n");
    std::abort();
  }
  return static_cast<uint64_t>(g_top->result);
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
