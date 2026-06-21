#include "verilator.h"
#include "VBBSimHarness.h"
#include "VBBSimHarness___024root.h"
#include "verilated.h"
#include "verilated_fst_c.h"

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <deque>
#include <cstring>
#include <mutex>
#include <unordered_map>
#include <vector>

#if VM_COVERAGE
#include "verilated_cov.h"
#endif

// Context management
extern "C" void *verilator_context_new() { return new VerilatedContext; }

extern "C" void verilator_context_free(void *ctx) {
  delete static_cast<VerilatedContext *>(ctx);
}

extern "C" void verilator_context_time_inc(void *ctx, uint64_t add) {
  static_cast<VerilatedContext *>(ctx)->timeInc(add);
}

extern "C" uint64_t verilator_context_time(void *ctx) {
  return static_cast<VerilatedContext *>(ctx)->time();
}

extern "C" void verilator_context_command_args(void *ctx, int argc,
                                               const char **argv) {
  static_cast<VerilatedContext *>(ctx)->commandArgs(argc,
                                                    const_cast<char **>(argv));
}

extern "C" void verilator_context_trace_ever_on(void *ctx, bool on) {
  static_cast<VerilatedContext *>(ctx)->traceEverOn(on);
}

extern "C" void verilator_context_coverage_write(void *ctx) {
#if VM_COVERAGE
  auto *context = static_cast<VerilatedContext *>(ctx);
  if (context->coveragep()) {
    context->coveragep()->write();
  }
#endif
}

// Top module
extern "C" void *verilator_top_new(void *ctx) {
  return new VBBSimHarness{static_cast<VerilatedContext *>(ctx)};
}

extern "C" void verilator_top_free(void *top) {
  delete static_cast<VBBSimHarness *>(top);
}

extern "C" void verilator_top_eval(void *top) {
  static_cast<VBBSimHarness *>(top)->eval();
}

extern "C" void verilator_top_trace(void *top, void *tfp, int levels) {
  static_cast<VBBSimHarness *>(top)->trace(static_cast<VerilatedFstC *>(tfp),
                                           levels);
}

namespace {

constexpr uint32_t kPrivateBankCount = 32;
// The generated private SRAM arrays in this RTL config are 128 rows of 128 bits.
// Callers may pass the larger unified BEMU bank buffer; bytes past the RTL array are zero-filled.
constexpr uint32_t kPrivateBankRows = 128;
constexpr uint32_t kPrivateBankWordsPerRow = 4;
constexpr uint32_t kPrivateBankBytes = kPrivateBankRows * kPrivateBankWordsPerRow * sizeof(uint32_t);

template <typename BankMemory>
void copy_private_bank(BankMemory &memory, uint8_t *out) {
  for (uint32_t row = 0; row < kPrivateBankRows; ++row) {
    for (uint32_t word = 0; word < kPrivateBankWordsPerRow; ++word) {
      const uint32_t value = memory[row][word];
      const uint32_t offset = (row * kPrivateBankWordsPerRow + word) * sizeof(uint32_t);
      out[offset + 0] = static_cast<uint8_t>(value & 0xffu);
      out[offset + 1] = static_cast<uint8_t>((value >> 8) & 0xffu);
      out[offset + 2] = static_cast<uint8_t>((value >> 16) & 0xffu);
      out[offset + 3] = static_cast<uint8_t>((value >> 24) & 0xffu);
    }
  }
}

} // namespace

#define COPY_PRIVATE_BANK_CASE(ID)                                                                  \
  case ID:                                                                                          \
    copy_private_bank(                                                                              \
        root->BBSimHarness__DOT__chiptop0__DOT__system__DOT__tile_prci_domain__DOT__element_reset_domain_bbtile__DOT__accelerators_0__DOT__memDomain__DOT__backend__DOT__privateBackend__DOT__banks_##ID##__DOT__mem_ext__DOT__Memory, \
        out);                                                                                       \
    return true

extern "C" bool verilator_read_private_bank(void *top, uint32_t bank_id, uint8_t *out,
                                             uint32_t out_len) {
  if (top == nullptr || out == nullptr || bank_id >= kPrivateBankCount ||
      out_len < kPrivateBankBytes) {
    return false;
  }

  auto *root = static_cast<VBBSimHarness *>(top)->rootp;
  if (root == nullptr) {
    return false;
  }

  std::memset(out, 0, out_len);
  switch (bank_id) {
    COPY_PRIVATE_BANK_CASE(0);
    COPY_PRIVATE_BANK_CASE(1);
    COPY_PRIVATE_BANK_CASE(2);
    COPY_PRIVATE_BANK_CASE(3);
    COPY_PRIVATE_BANK_CASE(4);
    COPY_PRIVATE_BANK_CASE(5);
    COPY_PRIVATE_BANK_CASE(6);
    COPY_PRIVATE_BANK_CASE(7);
    COPY_PRIVATE_BANK_CASE(8);
    COPY_PRIVATE_BANK_CASE(9);
    COPY_PRIVATE_BANK_CASE(10);
    COPY_PRIVATE_BANK_CASE(11);
    COPY_PRIVATE_BANK_CASE(12);
    COPY_PRIVATE_BANK_CASE(13);
    COPY_PRIVATE_BANK_CASE(14);
    COPY_PRIVATE_BANK_CASE(15);
    COPY_PRIVATE_BANK_CASE(16);
    COPY_PRIVATE_BANK_CASE(17);
    COPY_PRIVATE_BANK_CASE(18);
    COPY_PRIVATE_BANK_CASE(19);
    COPY_PRIVATE_BANK_CASE(20);
    COPY_PRIVATE_BANK_CASE(21);
    COPY_PRIVATE_BANK_CASE(22);
    COPY_PRIVATE_BANK_CASE(23);
    COPY_PRIVATE_BANK_CASE(24);
    COPY_PRIVATE_BANK_CASE(25);
    COPY_PRIVATE_BANK_CASE(26);
    COPY_PRIVATE_BANK_CASE(27);
    COPY_PRIVATE_BANK_CASE(28);
    COPY_PRIVATE_BANK_CASE(29);
    COPY_PRIVATE_BANK_CASE(30);
    COPY_PRIVATE_BANK_CASE(31);
  default:
    return false;
  }
}

#undef COPY_PRIVATE_BANK_CASE

#define READ_SCOREBOARD_CASE(ID)                                                                    \
  case ID:                                                                                          \
    *rd_count = 0;                                                                                  \
    *wr_busy =                                                                                      \
        root->BBSimHarness__DOT__chiptop0__DOT__system__DOT__tile_prci_domain__DOT__element_reset_domain_bbtile__DOT__accelerators_0__DOT__frontend__DOT__scheduler__DOT__rob__DOT__scoreboard__DOT__bankWrBusy_##ID; \
    return true

extern "C" bool verilator_read_bank_scoreboard(void *top, uint32_t bank_id,
                                               uint32_t *rd_count,
                                               bool *wr_busy) {
  if (top == nullptr || rd_count == nullptr || wr_busy == nullptr ||
      bank_id >= kPrivateBankCount) {
    return false;
  }

  auto *root = static_cast<VBBSimHarness *>(top)->rootp;
  if (root == nullptr) {
    return false;
  }

  switch (bank_id) {
    READ_SCOREBOARD_CASE(0);
    READ_SCOREBOARD_CASE(1);
    READ_SCOREBOARD_CASE(2);
    READ_SCOREBOARD_CASE(3);
    READ_SCOREBOARD_CASE(4);
    READ_SCOREBOARD_CASE(5);
    READ_SCOREBOARD_CASE(6);
    READ_SCOREBOARD_CASE(7);
    READ_SCOREBOARD_CASE(8);
    READ_SCOREBOARD_CASE(9);
    READ_SCOREBOARD_CASE(10);
    READ_SCOREBOARD_CASE(11);
    READ_SCOREBOARD_CASE(12);
    READ_SCOREBOARD_CASE(13);
    READ_SCOREBOARD_CASE(14);
    READ_SCOREBOARD_CASE(15);
    READ_SCOREBOARD_CASE(16);
    READ_SCOREBOARD_CASE(17);
    READ_SCOREBOARD_CASE(18);
    READ_SCOREBOARD_CASE(19);
    READ_SCOREBOARD_CASE(20);
    READ_SCOREBOARD_CASE(21);
    READ_SCOREBOARD_CASE(22);
    READ_SCOREBOARD_CASE(23);
    READ_SCOREBOARD_CASE(24);
    READ_SCOREBOARD_CASE(25);
    READ_SCOREBOARD_CASE(26);
    READ_SCOREBOARD_CASE(27);
    READ_SCOREBOARD_CASE(28);
    READ_SCOREBOARD_CASE(29);
    READ_SCOREBOARD_CASE(30);
    READ_SCOREBOARD_CASE(31);
  default:
    return false;
  }
}

#undef READ_SCOREBOARD_CASE

// Top module signals
extern "C" void verilator_top_set_clock(void *top, uint8_t val) {
  static_cast<VBBSimHarness *>(top)->clock = val;
}

extern "C" void verilator_top_set_reset(void *top, uint8_t val) {
  static_cast<VBBSimHarness *>(top)->reset = val;
}

// =============================================================================
// SCU shared state (used by both DPI-C callbacks and MMIO tick polling)
// =============================================================================
#define SIM_EXIT_ADDR 0x60000000ULL
#define UART_TX_ADDR 0x60020000ULL

static std::vector<uint32_t> g_uart_tx;
static std::unordered_map<uint32_t, std::deque<uint8_t>> g_uart_rx;
static int32_t g_exit_code = 0;
static bool g_has_exit = false;
static std::mutex g_scu_mutex;

// =============================================================================
// SCU DPI-C interface
// Called from RTL via DPI-C when software writes to SCU registers
// (0x6000_0000 for sim_exit, 0x6002_0000 for UART).
// State is queryable from Rust via verilator_scu_*() helpers below.
//
// Note: Verilator uses DPI-C SCU (per-tile UART/exit), not AXI4 MMIO.
// The SCU is intercepted inside each BBTile and never reaches the system bus.
// =============================================================================

extern "C" void scu_uart_write(uint32_t hart_id, uint32_t ch) {
  std::lock_guard<std::mutex> lock(g_scu_mutex);
  g_uart_tx.push_back(((hart_id & 0x00ffffffu) << 8) | (ch & 0xffu));
}

extern "C" int scu_uart_rx_valid(uint32_t hart_id) {
  std::lock_guard<std::mutex> lock(g_scu_mutex);
  auto it = g_uart_rx.find(hart_id);
  return it != g_uart_rx.end() && !it->second.empty();
}

extern "C" void scu_uart_rx_sample(uint32_t hart_id, uint32_t pop,
                                   uint32_t *valid, uint32_t *data) {
  std::lock_guard<std::mutex> lock(g_scu_mutex);
  if (valid == nullptr || data == nullptr) {
    fprintf(stderr, "scu_uart_rx_sample received null output pointer\n");
    abort();
  }

  auto it = g_uart_rx.find(hart_id);
  if (it == g_uart_rx.end() || it->second.empty()) {
    *valid = 0;
    *data = 0;
    return;
  }

  *valid = 1;
  *data = it->second.front();
  if (pop) {
    it->second.pop_front();
  }
}

extern "C" int scu_uart_peek(uint32_t hart_id) {
  std::lock_guard<std::mutex> lock(g_scu_mutex);
  auto it = g_uart_rx.find(hart_id);
  if (it == g_uart_rx.end() || it->second.empty()) {
    return 0;
  }
  return it->second.front();
}

extern "C" int scu_uart_pop(uint32_t hart_id) {
  std::lock_guard<std::mutex> lock(g_scu_mutex);
  auto it = g_uart_rx.find(hart_id);
  if (it == g_uart_rx.end() || it->second.empty()) {
    return 0;
  }
  uint8_t byte = it->second.front();
  it->second.pop_front();
  return byte;
}

extern "C" void scu_sim_exit(uint32_t hart_id, uint32_t code) {
  std::lock_guard<std::mutex> lock(g_scu_mutex);
  g_exit_code = code;
  g_has_exit = true;
  printf("\n[SCU] sim_exit called: hart_id=%u, exit_code=%u\n", hart_id, code);
  fflush(stdout);
}

extern "C" bool verilator_scu_has_exit() {
  std::lock_guard<std::mutex> lock(g_scu_mutex);
  return g_has_exit;
}

extern "C" int32_t verilator_scu_exit_code() {
  std::lock_guard<std::mutex> lock(g_scu_mutex);
  return g_exit_code;
}

extern "C" void verilator_scu_push_uart_rx(uint32_t hart_id, uint32_t byte) {
  std::lock_guard<std::mutex> lock(g_scu_mutex);
  g_uart_rx[hart_id].push_back((uint8_t)(byte & 0xff));
}

extern "C" uint32_t verilator_scu_drain_uart_tx(uint32_t *buf, uint32_t len) {
  std::lock_guard<std::mutex> lock(g_scu_mutex);
  uint32_t n = 0;
  while (n < len && n < g_uart_tx.size()) {
    buf[n] = g_uart_tx[n];
    n++;
  }
  g_uart_tx.erase(g_uart_tx.begin(), g_uart_tx.begin() + n);
  return n;
}

extern "C" void *verilator_trace_new() { return new VerilatedFstC; }

extern "C" void verilator_trace_free(void *tfp) {
  delete static_cast<VerilatedFstC *>(tfp);
}

extern "C" bool verilator_trace_open(void *tfp, const char *filename) {
  static_cast<VerilatedFstC *>(tfp)->open(filename);
  return true;
}

extern "C" void verilator_trace_dump(void *tfp, uint64_t timeui) {
  static_cast<VerilatedFstC *>(tfp)->dump(timeui);
}

extern "C" void verilator_trace_close(void *tfp) {
  static_cast<VerilatedFstC *>(tfp)->close();
}
