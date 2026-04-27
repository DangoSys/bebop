#include "processor.h"
#include "mmu.h"
#include "cfg.h"
#include "extension.h"
#include "simif.h"
#include "trap.h"
#include <vector>
#include <string>
#include <cstring>
#include <cstdio>

#define SIM_EXIT_ADDR 0x60000000UL
#define DRAM_BASE 0x80000000UL

extern "C" {
    uint64_t handle_syscall_ffi(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
    bool should_exit();
    int get_exit_code_ffi();
}

// Simple simif implementation without HTIF
class simple_simif_t : public simif_t {
public:
    simple_simif_t(uint8_t* mem_ptr, size_t mem_size, const char* isa_str)
        : mem_ptr(mem_ptr), mem_size(mem_size) {
        cfg.isa = isa_str;
        cfg.priv = "MSU";
        cfg.mem_layout.push_back(mem_cfg_t(DRAM_BASE, mem_size));
        cfg.hartids.push_back(0);
    }

    char* addr_to_mem(reg_t addr) override {
        if (addr >= DRAM_BASE && addr < DRAM_BASE + mem_size) {
            return reinterpret_cast<char*>(mem_ptr + (addr - DRAM_BASE));
        }
        return nullptr;
    }

    bool mmio_load(reg_t, size_t, uint8_t*) override {
        return false;
    }

    bool mmio_store(reg_t addr, size_t, const uint8_t*) override {
        if (addr == SIM_EXIT_ADDR) {
            exit_requested = true;
            return true;
        }
        return false;
    }

    void proc_reset(unsigned) override {}

    const cfg_t &get_cfg() const override {
        return cfg;
    }

    const std::map<size_t, processor_t*>& get_harts() const override {
        static std::map<size_t, processor_t*> empty_map;
        return empty_map;
    }

    const char* get_symbol(uint64_t) override {
        return nullptr;
    }

    bool exit_requested = false;

private:
    uint8_t* mem_ptr;
    size_t mem_size;
    cfg_t cfg;
};

extern "C" {

int spike_run_raw(
    const char* isa,
    size_t,
    uint8_t* mem_ptr,
    size_t mem_size,
    uint64_t entry,
    const char* log_path
) {
    // Create simif with Rust-provided memory
    simple_simif_t simif(mem_ptr, mem_size, isa);

    // Create processor
    FILE* log_file = nullptr;
    if (log_path && strlen(log_path) > 0) {
        log_file = fopen(log_path, "w");
    }

    processor_t proc(isa, "MSU", &simif.get_cfg(), &simif, 0, false, log_file, std::cerr);

    // Load buckyball extension
    auto ext_factory = find_extension("buckyball");
    if (ext_factory) {
        proc.register_extension(ext_factory());
    }

    // Set PC to entry point (provided by Rust)
    proc.get_state()->pc = entry;

    // Run until sim_exit or syscall exit
    while (!simif.exit_requested && !should_exit()) {
        try {
            proc.step(1);
        } catch (trap_t& t) {
            // Handle traps (exceptions)
            if (t.cause() == CAUSE_USER_ECALL ||
                t.cause() == CAUSE_SUPERVISOR_ECALL ||
                t.cause() == CAUSE_MACHINE_ECALL) {
                // System call - get arguments from registers
                state_t* state = proc.get_state();
                uint64_t syscall_num = state->XPR[17]; // a7
                uint64_t a0 = state->XPR[10];
                uint64_t a1 = state->XPR[11];
                uint64_t a2 = state->XPR[12];
                uint64_t a3 = state->XPR[13];
                uint64_t a4 = state->XPR[14];
                uint64_t a5 = state->XPR[15];

                // Call Rust syscall handler
                uint64_t result = handle_syscall_ffi(syscall_num, a0, a1, a2, a3, a4, a5);

                // Set return value
                state->XPR.write(10, result); // a0

                // Advance PC past ecall instruction
                state->pc += 4;
            } else {
                // Other trap - re-throw, will crash
                throw;
            }
        }
    }

    if (log_file) {
        fclose(log_file);
    }

    return should_exit() ? get_exit_code_ffi() : 0;
}
}
