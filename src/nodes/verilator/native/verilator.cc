// Minimal C++ wrapper for Verilator - exposes only essential APIs to Rust FFI

#include "verilator.h"
#include "VBBSimHarness.h"
#include "verilated.h"
#include "verilated_fst_c.h"

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

// MMIO signals
extern "C" uint8_t verilator_top_get_mmio_fire(void* top) {
    return static_cast<VBBSimHarness*>(top)->io_mmio_fire;
}

extern "C" uint64_t verilator_top_get_mmio_fire_addr(void* top) {
    return static_cast<VBBSimHarness*>(top)->io_mmio_fire_addr;
}

extern "C" uint64_t verilator_top_get_mmio_fire_data(void* top) {
    return static_cast<VBBSimHarness*>(top)->io_mmio_fire_data;
}

// FST trace
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
