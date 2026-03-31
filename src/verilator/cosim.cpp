// Drives `bebop_accel`: holds RoCC fields, pulses `issue_start` until `issue_done`, clocks TL +
// subsystem.

#include <cstdint>
#include <cstdio>
#include <cstdlib>

#include "Vbebop_accel.h"
#include "verilated.h"

static VerilatedContext *g_ctx;
static Vbebop_accel *g_top;

static uint32_t g_digest_all_banks = 0;
static void tick(void);

extern "C" void bebop_cosim_init(void) {
  if (g_top) {
    return;
  }
  g_ctx = new VerilatedContext;
  static char arg0[] = "bebop-verilator";
  static char *argv[] = {arg0, nullptr};
  g_ctx->commandArgs(1, argv);
  g_top = new Vbebop_accel{g_ctx};
  g_top->clk = 0;
  g_top->digest_all_banks = 0;
  g_top->issue_start = 0;
  g_top->eval();
  for (int i = 0; i < 32; i++) {
    tick();
  }
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

static void tick(void) {
  g_top->clk = 0;
  g_top->eval();
  g_top->clk = 1;
  g_top->eval();
  g_top->clk = 0;
  g_top->eval();
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
  g_top->issue_start = 1;
  tick();

  uint32_t guard = 2000000u;
  while (guard-- > 0) {
    if (g_top->issue_done) {
      break;
    }
    tick();
  }
  if (!g_top->issue_done) {
    std::fprintf(stderr, "bebop_cosim_issue: timeout funct=%u\n", funct & 0x7fU);
    std::abort();
  }

  g_top->issue_start = 0;
  tick();

  uint32_t qwait = 10000000u;
  while (qwait-- > 0 && g_top->rtl_busy) {
    tick();
  }
  if (g_top->rtl_busy) {
    std::fprintf(stderr, "bebop_cosim_issue: rtl still busy funct=%u\n", funct & 0x7fU);
    std::abort();
  }

  for (int i = 0; i < 512; i++) {
    tick();
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

extern "C" void dpi_itrace(unsigned char is_issue, unsigned int rob_id, unsigned int domain_id,
                           unsigned int funct, unsigned long long pc, unsigned long long rs1,
                           unsigned long long rs2, unsigned char bank_enable) {
  (void)is_issue;
  (void)rob_id;
  (void)domain_id;
  (void)funct;
  (void)pc;
  (void)rs1;
  (void)rs2;
  (void)bank_enable;
}

extern "C" void dpi_mtrace(unsigned char is_write, unsigned char is_shared, unsigned int channel,
                           unsigned long long hart_id, unsigned int vbank_id, unsigned int group_id,
                           unsigned int addr, unsigned long long data_lo,
                           unsigned long long data_hi) {
  (void)is_write;
  (void)is_shared;
  (void)channel;
  (void)hart_id;
  (void)vbank_id;
  (void)group_id;
  (void)addr;
  (void)data_lo;
  (void)data_hi;
}

extern "C" void dpi_pmctrace(unsigned int ball_id, unsigned int rob_id,
                             unsigned long long elapsed) {
  (void)ball_id;
  (void)rob_id;
  (void)elapsed;
}

extern "C" void dpi_mem_pmctrace(unsigned char is_store, unsigned int rob_id,
                                 unsigned long long elapsed) {
  (void)is_store;
  (void)rob_id;
  (void)elapsed;
}

extern "C" void dpi_ctrace(unsigned char subcmd, unsigned int ctr_id, unsigned long long tag,
                           unsigned long long elapsed, unsigned long long cycle) {
  (void)subcmd;
  (void)ctr_id;
  (void)tag;
  (void)elapsed;
  (void)cycle;
}

extern "C" unsigned long long dpi_backdoor_get_read_addr(void) { return 0ULL; }

extern "C" unsigned long long dpi_backdoor_get_write_addr(void) { return 0ULL; }

extern "C" void dpi_backdoor_get_write_data(unsigned long long *data_lo,
                                            unsigned long long *data_hi) {
  *data_lo = 0ULL;
  *data_hi = 0ULL;
}

extern "C" void dpi_backdoor_put_read_data(unsigned int bank_id, unsigned int row,
                                           unsigned long long data_lo, unsigned long long data_hi) {
  (void)bank_id;
  (void)row;
  (void)data_lo;
  (void)data_hi;
}

extern "C" void dpi_backdoor_put_write_done(unsigned int bank_id, unsigned int row,
                                            unsigned long long data_lo,
                                            unsigned long long data_hi) {
  (void)bank_id;
  (void)row;
  (void)data_lo;
  (void)data_hi;
}
