#ifndef BEBOP_VERILATOR_H_
#define BEBOP_VERILATOR_H_

#include <stdint.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

// Verilator context management
void* verilator_context_new();
void verilator_context_free(void* ctx);
void verilator_context_time_inc(void* ctx, uint64_t add);
uint64_t verilator_context_time(void* ctx);
void verilator_context_command_args(void* ctx, int argc, const char** argv);
void verilator_context_trace_ever_on(void* ctx, bool on);
void verilator_context_coverage_write(void* ctx);

// Top module
void* verilator_top_new(void* ctx);
void verilator_top_free(void* top);
void verilator_top_eval(void* top);
void verilator_top_trace(void* top, void* tfp, int levels);

// Top module signals
void verilator_top_set_clock(void* top, uint8_t val);
void verilator_top_set_reset(void* top, uint8_t val);

// SCU state query (DPI-C functions are called from RTL automatically)
bool verilator_scu_has_exit();

// FST trace
void* verilator_trace_new();
void verilator_trace_free(void* tfp);
bool verilator_trace_open(void* tfp, const char* filename);
void verilator_trace_dump(void* tfp, uint64_t timeui);
void verilator_trace_close(void* tfp);

#ifdef __cplusplus
}
#endif

#endif // BEBOP_VERILATOR_H_
