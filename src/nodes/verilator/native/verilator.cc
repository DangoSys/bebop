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
constexpr uint32_t kGlobalRobEntryCount = 16;

template <typename BankMemory>
void copy_private_bank(BankMemory &memory, uint8_t *out) {
  constexpr uint32_t rows = sizeof(memory) / sizeof(memory[0]);
  constexpr uint32_t words_per_row = sizeof(memory[0]) / sizeof(memory[0][0]);
  for (uint32_t row = 0; row < rows; ++row) {
    for (uint32_t word = 0; word < words_per_row; ++word) {
      const uint32_t value = memory[row][word];
      const uint32_t offset = (row * words_per_row + word) * sizeof(uint32_t);
      out[offset + 0] = static_cast<uint8_t>(value & 0xffu);
      out[offset + 1] = static_cast<uint8_t>((value >> 8) & 0xffu);
      out[offset + 2] = static_cast<uint8_t>((value >> 16) & 0xffu);
      out[offset + 3] = static_cast<uint8_t>((value >> 24) & 0xffu);
    }
  }
}

template <typename BankMemory>
uint64_t hash_private_bank(BankMemory &memory) {
  constexpr uint64_t kFnv1a64OffsetBasis = 0xcbf29ce484222325ULL;
  constexpr uint64_t kFnv1a64Prime = 0x00000100000001b3ULL;
  uint64_t hash = kFnv1a64OffsetBasis;
  constexpr uint32_t rows = sizeof(memory) / sizeof(memory[0]);
  constexpr uint32_t words_per_row = sizeof(memory[0]) / sizeof(memory[0][0]);
  for (uint32_t row = 0; row < rows; ++row) {
    for (uint32_t word = 0; word < words_per_row; ++word) {
      const uint32_t value = memory[row][word];
      for (uint32_t byte = 0; byte < sizeof(uint32_t); ++byte) {
        hash ^= static_cast<uint8_t>((value >> (byte * 8)) & 0xffu);
        hash *= kFnv1a64Prime;
      }
    }
  }
  return hash;
}

template <typename BankMemory>
bool flip_private_bank_bit(BankMemory &memory, uint32_t byte_offset,
                           uint8_t bit) {
  constexpr uint32_t rows = sizeof(memory) / sizeof(memory[0]);
  constexpr uint32_t words_per_row = sizeof(memory[0]) / sizeof(memory[0][0]);
  constexpr uint32_t bank_bytes = rows * words_per_row * sizeof(uint32_t);
  if (byte_offset >= bank_bytes || bit >= 8) {
    return false;
  }
  const uint32_t word_index = byte_offset / sizeof(uint32_t);
  const uint32_t row = word_index / words_per_row;
  const uint32_t word = word_index % words_per_row;
  const uint32_t bit_in_word = (byte_offset % sizeof(uint32_t)) * 8 + bit;
  memory[row][word] ^= uint32_t{1} << bit_in_word;
  return true;
}

} // namespace

extern "C" uint32_t verilator_private_bank_count() {
  return kPrivateBankCount;
}

extern "C" uint32_t verilator_private_bank_bytes(void *top) {
  if (top == nullptr) {
    return 0;
  }
  auto *root = static_cast<VBBSimHarness *>(top)->rootp;
  if (root == nullptr) {
    return 0;
  }
  return sizeof(
      root->BBSimHarness__DOT__chiptop0__DOT__system__DOT__tile_prci_domain__DOT__element_reset_domain_bbtile__DOT__accelerators_0__DOT__memDomain__DOT__backend__DOT__privateBackend__DOT__banks_0__DOT__mem_ext__DOT__Memory);
}

#define COPY_PRIVATE_BANK_CASE(ID)                                                                  \
  case ID:                                                                                          \
    copy_private_bank(                                                                              \
        root->BBSimHarness__DOT__chiptop0__DOT__system__DOT__tile_prci_domain__DOT__element_reset_domain_bbtile__DOT__accelerators_0__DOT__memDomain__DOT__backend__DOT__privateBackend__DOT__banks_##ID##__DOT__mem_ext__DOT__Memory, \
        out);                                                                                       \
    return true

extern "C" bool verilator_read_private_bank(void *top, uint32_t bank_id, uint8_t *out,
                                             uint32_t out_len) {
  if (top == nullptr || out == nullptr || bank_id >= kPrivateBankCount) {
    return false;
  }

  auto *root = static_cast<VBBSimHarness *>(top)->rootp;
  if (root == nullptr) {
    return false;
  }
  const uint32_t bank_bytes = sizeof(
      root->BBSimHarness__DOT__chiptop0__DOT__system__DOT__tile_prci_domain__DOT__element_reset_domain_bbtile__DOT__accelerators_0__DOT__memDomain__DOT__backend__DOT__privateBackend__DOT__banks_0__DOT__mem_ext__DOT__Memory);
  if (out_len < bank_bytes) {
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

#define HASH_PRIVATE_BANK_CASE(ID)                                                                  \
  case ID:                                                                                          \
    *out_hash = hash_private_bank(                                                                  \
        root->BBSimHarness__DOT__chiptop0__DOT__system__DOT__tile_prci_domain__DOT__element_reset_domain_bbtile__DOT__accelerators_0__DOT__memDomain__DOT__backend__DOT__privateBackend__DOT__banks_##ID##__DOT__mem_ext__DOT__Memory); \
    return true

extern "C" bool verilator_hash_private_bank(void *top, uint32_t bank_id,
                                             uint64_t *out_hash) {
  if (top == nullptr || out_hash == nullptr || bank_id >= kPrivateBankCount) {
    return false;
  }
  auto *root = static_cast<VBBSimHarness *>(top)->rootp;
  if (root == nullptr) {
    return false;
  }
  switch (bank_id) {
    HASH_PRIVATE_BANK_CASE(0); HASH_PRIVATE_BANK_CASE(1);
    HASH_PRIVATE_BANK_CASE(2); HASH_PRIVATE_BANK_CASE(3);
    HASH_PRIVATE_BANK_CASE(4); HASH_PRIVATE_BANK_CASE(5);
    HASH_PRIVATE_BANK_CASE(6); HASH_PRIVATE_BANK_CASE(7);
    HASH_PRIVATE_BANK_CASE(8); HASH_PRIVATE_BANK_CASE(9);
    HASH_PRIVATE_BANK_CASE(10); HASH_PRIVATE_BANK_CASE(11);
    HASH_PRIVATE_BANK_CASE(12); HASH_PRIVATE_BANK_CASE(13);
    HASH_PRIVATE_BANK_CASE(14); HASH_PRIVATE_BANK_CASE(15);
    HASH_PRIVATE_BANK_CASE(16); HASH_PRIVATE_BANK_CASE(17);
    HASH_PRIVATE_BANK_CASE(18); HASH_PRIVATE_BANK_CASE(19);
    HASH_PRIVATE_BANK_CASE(20); HASH_PRIVATE_BANK_CASE(21);
    HASH_PRIVATE_BANK_CASE(22); HASH_PRIVATE_BANK_CASE(23);
    HASH_PRIVATE_BANK_CASE(24); HASH_PRIVATE_BANK_CASE(25);
    HASH_PRIVATE_BANK_CASE(26); HASH_PRIVATE_BANK_CASE(27);
    HASH_PRIVATE_BANK_CASE(28); HASH_PRIVATE_BANK_CASE(29);
    HASH_PRIVATE_BANK_CASE(30); HASH_PRIVATE_BANK_CASE(31);
  default:
    return false;
  }
}

#undef HASH_PRIVATE_BANK_CASE

#define FLIP_PRIVATE_BANK_CASE(ID)                                                   \
  case ID:                                                                           \
    return flip_private_bank_bit(                                                    \
        root->BBSimHarness__DOT__chiptop0__DOT__system__DOT__tile_prci_domain__DOT__element_reset_domain_bbtile__DOT__accelerators_0__DOT__memDomain__DOT__backend__DOT__privateBackend__DOT__banks_##ID##__DOT__mem_ext__DOT__Memory, \
        byte_offset, bit)

extern "C" bool verilator_flip_private_bank_bit(void *top, uint32_t bank_id,
                                                 uint32_t byte_offset,
                                                 uint8_t bit) {
  if (top == nullptr || bank_id >= kPrivateBankCount || bit >= 8) {
    return false;
  }
  auto *root = static_cast<VBBSimHarness *>(top)->rootp;
  if (root == nullptr) {
    return false;
  }
  switch (bank_id) {
    FLIP_PRIVATE_BANK_CASE(0); FLIP_PRIVATE_BANK_CASE(1);
    FLIP_PRIVATE_BANK_CASE(2); FLIP_PRIVATE_BANK_CASE(3);
    FLIP_PRIVATE_BANK_CASE(4); FLIP_PRIVATE_BANK_CASE(5);
    FLIP_PRIVATE_BANK_CASE(6); FLIP_PRIVATE_BANK_CASE(7);
    FLIP_PRIVATE_BANK_CASE(8); FLIP_PRIVATE_BANK_CASE(9);
    FLIP_PRIVATE_BANK_CASE(10); FLIP_PRIVATE_BANK_CASE(11);
    FLIP_PRIVATE_BANK_CASE(12); FLIP_PRIVATE_BANK_CASE(13);
    FLIP_PRIVATE_BANK_CASE(14); FLIP_PRIVATE_BANK_CASE(15);
    FLIP_PRIVATE_BANK_CASE(16); FLIP_PRIVATE_BANK_CASE(17);
    FLIP_PRIVATE_BANK_CASE(18); FLIP_PRIVATE_BANK_CASE(19);
    FLIP_PRIVATE_BANK_CASE(20); FLIP_PRIVATE_BANK_CASE(21);
    FLIP_PRIVATE_BANK_CASE(22); FLIP_PRIVATE_BANK_CASE(23);
    FLIP_PRIVATE_BANK_CASE(24); FLIP_PRIVATE_BANK_CASE(25);
    FLIP_PRIVATE_BANK_CASE(26); FLIP_PRIVATE_BANK_CASE(27);
    FLIP_PRIVATE_BANK_CASE(28); FLIP_PRIVATE_BANK_CASE(29);
    FLIP_PRIVATE_BANK_CASE(30); FLIP_PRIVATE_BANK_CASE(31);
  default:
    return false;
  }
}

#undef FLIP_PRIVATE_BANK_CASE
// Read the authoritative vbank -> physical-Bank mapping maintained by the
// actual RTL private-memory backend. DiffTest must not reproduce the backend's
// allocation policy in software because that can silently drift from RTL.
#define ACCUMULATE_PRIVATE_MAPPING(ID)                                                         \
  do {                                                                                         \
    if (root->BBSimHarness__DOT__chiptop0__DOT__system__DOT__tile_prci_domain__DOT__element_reset_domain_bbtile__DOT__accelerators_0__DOT__memDomain__DOT__backend__DOT__privateBackend__DOT__mappingTable_##ID##_valid && \
        root->BBSimHarness__DOT__chiptop0__DOT__system__DOT__tile_prci_domain__DOT__element_reset_domain_bbtile__DOT__accelerators_0__DOT__memDomain__DOT__backend__DOT__privateBackend__DOT__mappingTable_##ID##_vbank_id == vbank_id) { \
      *pbank_mask |= (uint32_t{1} << ID);                                                       \
    }                                                                                          \
  } while (false)

extern "C" bool verilator_resolve_private_bank_mask(void *top,
                                                       uint32_t vbank_id,
                                                       uint32_t *pbank_mask) {
  if (top == nullptr || pbank_mask == nullptr || vbank_id >= kPrivateBankCount) {
    return false;
  }

  auto *root = static_cast<VBBSimHarness *>(top)->rootp;
  if (root == nullptr) {
    return false;
  }

  *pbank_mask = 0;
  ACCUMULATE_PRIVATE_MAPPING(0);
  ACCUMULATE_PRIVATE_MAPPING(1);
  ACCUMULATE_PRIVATE_MAPPING(2);
  ACCUMULATE_PRIVATE_MAPPING(3);
  ACCUMULATE_PRIVATE_MAPPING(4);
  ACCUMULATE_PRIVATE_MAPPING(5);
  ACCUMULATE_PRIVATE_MAPPING(6);
  ACCUMULATE_PRIVATE_MAPPING(7);
  ACCUMULATE_PRIVATE_MAPPING(8);
  ACCUMULATE_PRIVATE_MAPPING(9);
  ACCUMULATE_PRIVATE_MAPPING(10);
  ACCUMULATE_PRIVATE_MAPPING(11);
  ACCUMULATE_PRIVATE_MAPPING(12);
  ACCUMULATE_PRIVATE_MAPPING(13);
  ACCUMULATE_PRIVATE_MAPPING(14);
  ACCUMULATE_PRIVATE_MAPPING(15);
  ACCUMULATE_PRIVATE_MAPPING(16);
  ACCUMULATE_PRIVATE_MAPPING(17);
  ACCUMULATE_PRIVATE_MAPPING(18);
  ACCUMULATE_PRIVATE_MAPPING(19);
  ACCUMULATE_PRIVATE_MAPPING(20);
  ACCUMULATE_PRIVATE_MAPPING(21);
  ACCUMULATE_PRIVATE_MAPPING(22);
  ACCUMULATE_PRIVATE_MAPPING(23);
  ACCUMULATE_PRIVATE_MAPPING(24);
  ACCUMULATE_PRIVATE_MAPPING(25);
  ACCUMULATE_PRIVATE_MAPPING(26);
  ACCUMULATE_PRIVATE_MAPPING(27);
  ACCUMULATE_PRIVATE_MAPPING(28);
  ACCUMULATE_PRIVATE_MAPPING(29);
  ACCUMULATE_PRIVATE_MAPPING(30);
  ACCUMULATE_PRIVATE_MAPPING(31);
  return true;
}

#undef ACCUMULATE_PRIVATE_MAPPING

#define READ_ROB_BANK_ACCESS_CASE(ID)                                                          \
  case ID:                                                                                     \
    *rd0_valid =                                                                               \
        root->BBSimHarness__DOT__chiptop0__DOT__system__DOT__tile_prci_domain__DOT__element_reset_domain_bbtile__DOT__accelerators_0__DOT__frontend__DOT__scheduler__DOT__rob__DOT__robEntries_##ID##_cmd_bankAccess_rd_bank_0_valid; \
    *rd0_vbank_id =                                                                            \
        root->BBSimHarness__DOT__chiptop0__DOT__system__DOT__tile_prci_domain__DOT__element_reset_domain_bbtile__DOT__accelerators_0__DOT__frontend__DOT__scheduler__DOT__rob__DOT__robEntries_##ID##_cmd_bankAccess_rd_bank_0_id; \
    *rd1_valid =                                                                               \
        root->BBSimHarness__DOT__chiptop0__DOT__system__DOT__tile_prci_domain__DOT__element_reset_domain_bbtile__DOT__accelerators_0__DOT__frontend__DOT__scheduler__DOT__rob__DOT__robEntries_##ID##_cmd_bankAccess_rd_bank_1_valid; \
    *rd1_vbank_id =                                                                            \
        root->BBSimHarness__DOT__chiptop0__DOT__system__DOT__tile_prci_domain__DOT__element_reset_domain_bbtile__DOT__accelerators_0__DOT__frontend__DOT__scheduler__DOT__rob__DOT__robEntries_##ID##_cmd_bankAccess_rd_bank_1_id; \
    *wr_valid =                                                                                \
        root->BBSimHarness__DOT__chiptop0__DOT__system__DOT__tile_prci_domain__DOT__element_reset_domain_bbtile__DOT__accelerators_0__DOT__frontend__DOT__scheduler__DOT__rob__DOT__robEntries_##ID##_cmd_bankAccess_wr_bank_valid; \
    *wr_vbank_id =                                                                             \
        root->BBSimHarness__DOT__chiptop0__DOT__system__DOT__tile_prci_domain__DOT__element_reset_domain_bbtile__DOT__accelerators_0__DOT__frontend__DOT__scheduler__DOT__rob__DOT__robEntries_##ID##_cmd_bankAccess_wr_bank_id; \
    return true

extern "C" bool verilator_read_rob_bank_access(
    void *top, uint32_t rob_id, bool *rd0_valid, uint32_t *rd0_vbank_id,
    bool *rd1_valid, uint32_t *rd1_vbank_id, bool *wr_valid,
    uint32_t *wr_vbank_id) {
  if (top == nullptr || rd0_valid == nullptr || rd0_vbank_id == nullptr ||
      rd1_valid == nullptr || rd1_vbank_id == nullptr || wr_valid == nullptr ||
      wr_vbank_id == nullptr ||
      rob_id >= kGlobalRobEntryCount) {
    return false;
  }
  auto *root = static_cast<VBBSimHarness *>(top)->rootp;
  if (root == nullptr) {
    return false;
  }

  switch (rob_id) {
    READ_ROB_BANK_ACCESS_CASE(0);
    READ_ROB_BANK_ACCESS_CASE(1);
    READ_ROB_BANK_ACCESS_CASE(2);
    READ_ROB_BANK_ACCESS_CASE(3);
    READ_ROB_BANK_ACCESS_CASE(4);
    READ_ROB_BANK_ACCESS_CASE(5);
    READ_ROB_BANK_ACCESS_CASE(6);
    READ_ROB_BANK_ACCESS_CASE(7);
    READ_ROB_BANK_ACCESS_CASE(8);
    READ_ROB_BANK_ACCESS_CASE(9);
    READ_ROB_BANK_ACCESS_CASE(10);
    READ_ROB_BANK_ACCESS_CASE(11);
    READ_ROB_BANK_ACCESS_CASE(12);
    READ_ROB_BANK_ACCESS_CASE(13);
    READ_ROB_BANK_ACCESS_CASE(14);
    READ_ROB_BANK_ACCESS_CASE(15);
  default:
    return false;
  }
}

#undef READ_ROB_BANK_ACCESS_CASE


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
