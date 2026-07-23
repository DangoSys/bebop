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

// Top module
void* verilator_top_new(void* ctx);
void verilator_top_free(void* top);
void verilator_top_eval(void* top);
void verilator_top_trace(void* top, void* tfp, int levels);
uint32_t verilator_private_bank_count();
uint32_t verilator_private_bank_bytes(void* top);
bool verilator_read_private_bank(void* top, uint32_t bank_id, uint8_t* out, uint32_t out_len);
bool verilator_hash_private_bank(void* top, uint32_t bank_id, uint64_t* out_hash);
bool verilator_flip_private_bank_bit(void* top, uint32_t bank_id,
                                     uint32_t byte_offset, uint8_t bit);
bool verilator_resolve_private_bank_mask(void* top, uint32_t vbank_id, uint32_t* pbank_mask);
bool verilator_read_rob_bank_access(void* top, uint32_t rob_id,
                                    bool* rd0_valid, uint32_t* rd0_vbank_id,
                                    bool* rd1_valid, uint32_t* rd1_vbank_id,
                                    bool* wr_valid, uint32_t* wr_vbank_id);

// Top module signals
void verilator_top_set_clock(void* top, uint8_t val);
void verilator_top_set_reset(void* top, uint8_t val);

// SCU state query (DPI-C functions are called from RTL automatically)
bool verilator_scu_has_exit();
int32_t verilator_scu_exit_code();
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
