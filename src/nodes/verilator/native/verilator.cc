// Minimal C++ wrapper for Verilator - exposes only essential APIs to Rust FFI

#include "verilator.h"
#include "VBBSimHarness.h"
#include "verilated.h"
#include "verilated_fst_c.h"

#include <cstdio>
#include <cstdint>
#include <vector>
#include <mutex>
#include <string>

#if VM_COVERAGE
#include "verilated_cov.h"
#endif

// Context management
extern "C" void* verilator_context_new() {
    return new VerilatedContext;
}

extern "C" void verilator_context_free(void* ctx) {
    delete static_cast<VerilatedContext*>(ctx);
}

extern "C" void verilator_context_time_inc(void* ctx, uint64_t add) {
    static_cast<VerilatedContext*>(ctx)->timeInc(add);
}

extern "C" uint64_t verilator_context_time(void* ctx) {
    return static_cast<VerilatedContext*>(ctx)->time();
}

extern "C" void verilator_context_command_args(void* ctx, int argc, const char** argv) {
    static_cast<VerilatedContext*>(ctx)->commandArgs(argc, const_cast<char**>(argv));
}

extern "C" void verilator_context_trace_ever_on(void* ctx, bool on) {
    static_cast<VerilatedContext*>(ctx)->traceEverOn(on);
}

extern "C" void verilator_context_coverage_write(void* ctx) {
#if VM_COVERAGE
    auto* context = static_cast<VerilatedContext*>(ctx);
    if (context->coveragep()) {
        context->coveragep()->write();
    }
#endif
}

// Top module
extern "C" void* verilator_top_new(void* ctx) {
    return new VBBSimHarness{static_cast<VerilatedContext*>(ctx)};
}

extern "C" void verilator_top_free(void* top) {
    delete static_cast<VBBSimHarness*>(top);
}

extern "C" void verilator_top_eval(void* top) {
    static_cast<VBBSimHarness*>(top)->eval();
}

extern "C" void verilator_top_trace(void* top, void* tfp, int levels) {
    static_cast<VBBSimHarness*>(top)->trace(static_cast<VerilatedFstC*>(tfp), levels);
}

// Top module signals
extern "C" void verilator_top_set_clock(void* top, uint8_t val) {
    static_cast<VBBSimHarness*>(top)->clock = val;
}

extern "C" void verilator_top_set_reset(void* top, uint8_t val) {
    static_cast<VBBSimHarness*>(top)->reset = val;
}

extern "C" uint8_t verilator_top_get_clock(void* top) {
    return static_cast<VBBSimHarness*>(top)->clock;
}

extern "C" uint8_t verilator_top_get_reset(void* top) {
    return static_cast<VBBSimHarness*>(top)->reset;
}

<<<<<<< HEAD
//===-----------------------------------------------------------------===//=
// SCU DPI-C interface
// Called from RTL via DPI-C when software writes to SCU registers
// (0x6000_0000 for sim_exit, 0x6002_0000 for UART).
// State is queryable from Rust via verilator_scu_*() helpers below.
//===-----------------------------------------------------------------===//=
=======
// =============================================================================
// SCU shared state (used by both DPI-C callbacks and MMIO tick polling)
// =============================================================================
#define SIM_EXIT_ADDR 0x60000000ULL
#define UART_TX_ADDR  0x60020000ULL

>>>>>>> 96596c8... feat: implement MMIO tick function to read signals from BBSimHarness and update SCU state
static std::vector<uint8_t> g_uart_log;
static int32_t g_exit_code = 0;
static bool g_has_exit = false;
static std::mutex g_scu_mutex;
static std::string g_uart_line_buf;

static void uart_flush_line() {
    if (!g_uart_line_buf.empty()) {
        fwrite(g_uart_line_buf.data(), 1, g_uart_line_buf.size(), stdout);
        fputc('\n', stdout);
        fflush(stdout);
        g_uart_line_buf.clear();
    }
}

// =============================================================================
// MMIO tick: read io_mmio_fire signals from BBSimHarness RTL.
// WithBBSimMMIO binder latches AXI4 write address/data and produces a
// 1-cycle firePulse on io_mmio_fire.  This function samples those signals
// after each posedge eval and updates the global SCU state so that Rust
// can query it via verilator_scu_has_exit() / verilator_scu_exit_code().
//
// Rising-edge detection ensures each MMIO write is processed exactly once
// even if io_mmio_fire stays high for multiple cycles.
// UART output is line-buffered to avoid character stacking in terminal.
// =============================================================================
extern "C" void verilator_mmio_tick(void* top) {
    auto* t = static_cast<VBBSimHarness*>(top);
    bool cur_fire = t->io_mmio_fire;
    static bool prev_fire = false;
    bool rising = cur_fire && !prev_fire;
    prev_fire = cur_fire;
    if (!rising) return;

    uint64_t addr = (uint64_t)t->io_mmio_fire_addr;
    uint64_t data = (uint64_t)t->io_mmio_fire_data;

    std::lock_guard<std::mutex> lock(g_scu_mutex);
    if (addr == SIM_EXIT_ADDR) {
        uart_flush_line();
        int code = static_cast<int>(data & 0xFFFFFFFF);
        g_exit_code = code;
        g_has_exit = true;
        if (code == 0)
            fprintf(stderr, "[MMIO] simulation success\n");
        else
            fprintf(stderr, "[MMIO] simulation exit code %d\n", code);
        fflush(stderr);
    } else if (addr == UART_TX_ADDR) {
        char ch = static_cast<char>(data & 0xFF);
        g_uart_log.push_back(static_cast<uint8_t>(ch));
        if (ch == '\n') {
            uart_flush_line();
        } else {
            g_uart_line_buf.push_back(ch);
        }
    }
}

// =============================================================================
// SCU DPI-C interface
// Called from RTL via DPI-C when software writes to SCU registers
// (0x6000_0000 for sim_exit, 0x6002_0000 for UART).
// State is queryable from Rust via verilator_scu_*() helpers below.
// =============================================================================

extern "C" void scu_uart_write(uint32_t hart_id, uint32_t ch) {
    (void)hart_id;
    std::lock_guard<std::mutex> lock(g_scu_mutex);
<<<<<<< HEAD
    g_uart_log.push_back((uint8_t)(ch & 0xFF));
    putchar((char)(ch & 0xFF));
    fflush(stdout);
=======
    char c = (char)(ch & 0xFF);
    g_uart_log.push_back((uint8_t)c);
    if (c == '\n') {
        uart_flush_line();
    } else {
        g_uart_line_buf.push_back(c);
    }
    *ack = 1;
>>>>>>> 96596c8... feat: implement MMIO tick function to read signals from BBSimHarness and update SCU state
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

extern "C" void verilator_scu_reset() {
    std::lock_guard<std::mutex> lock(g_scu_mutex);
    uart_flush_line();
    g_uart_log.clear();
    g_exit_code = 0;
    g_has_exit = false;
}

extern "C" void* verilator_trace_new() {
    return new VerilatedFstC;
}

extern "C" void verilator_trace_free(void* tfp) {
    delete static_cast<VerilatedFstC*>(tfp);
}

extern "C" bool verilator_trace_open(void* tfp, const char* filename) {
    static_cast<VerilatedFstC*>(tfp)->open(filename);
    return true;
}

extern "C" void verilator_trace_dump(void* tfp, uint64_t timeui) {
    static_cast<VerilatedFstC*>(tfp)->dump(timeui);
}

extern "C" void verilator_trace_close(void* tfp) {
    static_cast<VerilatedFstC*>(tfp)->close();
}
