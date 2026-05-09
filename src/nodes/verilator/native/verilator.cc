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

// =============================================================================
// SCU DPI-C interface
// Called from RTL via DPI-C when software writes to SCU registers
// (0x6000_0000 for sim_exit, 0x6002_0000 for UART).
// State is queryable from Rust via verilator_scu_*() helpers below.
// =============================================================================
static std::vector<uint8_t> g_uart_log;
static int32_t g_exit_code = 0;
static bool g_has_exit = false;
static std::mutex g_scu_mutex;

extern "C" void scu_uart_write(uint32_t hart_id, uint32_t ch, unsigned char* ack) {
    std::lock_guard<std::mutex> lock(g_scu_mutex);
    g_uart_log.push_back((uint8_t)(ch & 0xFF));
    putchar((char)(ch & 0xFF));
    fflush(stdout);
    // give response to FPGA to continue running
    *ack = 1;  
}

extern "C" void scu_sim_exit(uint32_t hart_id, uint32_t code, unsigned char* ack) {
    std::lock_guard<std::mutex> lock(g_scu_mutex);
    g_exit_code = code;
    g_has_exit = true;
    printf("\n[SCU] sim_exit called: hart_id=%u, exit_code=%u\n", hart_id, code);
    fflush(stdout);
    // give response to FPGA to continue running
    *ack = 1;  
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
