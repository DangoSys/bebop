#include "processor.h"
#include "mmu.h"
#include "cfg.h"
#include "extension.h"
#include "simif.h"
#include <vector>
#include <string>
#include <cstring>
#include <cstdio>

#define SIM_EXIT_ADDR 0x60000000UL
#define DRAM_BASE 0x80000000UL

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
    try {
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

        // Run until sim_exit
        while (!simif.exit_requested) {
            proc.step(1);
        }

        if (log_file) {
            fclose(log_file);
        }

        return 0;
    } catch (const std::exception& e) {
        fprintf(stderr, "spike_run error: %s\n", e.what());
        return 1;
    }
}

}
