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
#include <cstdlib>
#include <csignal>
#include <execinfo.h>
#include <unistd.h>

#define SIM_EXIT_ADDR 0x60000000UL
#define DRAM_BASE 0x80000000UL
#define UART_BASE 0x60020000UL
#define UART_SIZE 0x100UL
#define LOW_ALIAS_BASE 0x10000UL

extern "C" {
    uint64_t handle_syscall_ffi(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t);
    bool should_exit();
    int get_exit_code_ffi();
    uint64_t uart_mmio_load(uint8_t* uart_ptr, uint64_t addr, size_t size);
    bool uart_mmio_store(uint8_t* uart_ptr, uint64_t addr, size_t size, uint64_t value);
}

static void crash_handler(int sig) {
    void* bt[64];
    int n = backtrace(bt, 64);
    fprintf(stderr, "[FATAL] signal=%d in spike_wrapper\n", sig);
    backtrace_symbols_fd(bt, n, STDERR_FILENO);
    fflush(stderr);
    _Exit(128 + sig);
}

// Forward declare the buckyball extension factory
// This is defined in rocc.cc via REGISTER_EXTENSION macro
extern std::function<extension_t*()> buckyball_extension_factory;

// Manually register buckyball extension to avoid dlopen() issues
static void ensure_buckyball_registered() {
    static bool registered = false;
    if (!registered) {
        register_extension("buckyball", buckyball_extension_factory);
        registered = true;
    }
}

// Simple simif implementation without HTIF
class simple_simif_t : public simif_t {
public:
    simple_simif_t(uint8_t* mem_ptr, size_t mem_size, uint8_t* uart_ptr, const char* isa_str)
        : mem_ptr(mem_ptr), mem_size(mem_size), uart_ptr(uart_ptr) {
        isa_storage = isa_str;
        if (isa_storage.find("xbuckyball") == std::string::npos) {
            isa_storage += "_xbuckyball";
        }
        // Enable Zicclsm extension to allow misaligned load/store accesses
        // (real Linux kernels handle misaligned traps in software; bemu has no such handler,
        // so we let Spike's MMU permit them directly, matching the FPGA Linux environment.)
        if (isa_storage.find("zicclsm") == std::string::npos) {
            isa_storage += "_zicclsm";
        }
        // Enable Zicntr (cycle/time/instret) and Zihpm (hardware perf counters) extensions
        // so guest code can execute `rdcycle`/`rdtime`/`rdinstret` instructions.
        // Without these, `csrr a5, cycle` (used by LeNet's read_cycles()) traps as illegal.
        if (isa_storage.find("zicntr") == std::string::npos) {
            isa_storage += "_zicntr";
        }
        if (isa_storage.find("zihpm") == std::string::npos) {
            isa_storage += "_zihpm";
        }
        cfg.isa = isa_storage.c_str();
        cfg.priv = "MSU";  // Support Machine, Supervisor, and User modes
        cfg.mem_layout.push_back(mem_cfg_t(DRAM_BASE, mem_size));
        cfg.hartids.push_back(0);
    }

    char* addr_to_mem(reg_t addr) override {
        if (addr >= DRAM_BASE && addr < DRAM_BASE + mem_size) {
            return reinterpret_cast<char*>(mem_ptr + (addr - DRAM_BASE));
        }
        // Alias low virtual addresses for relocated EXEC binaries.
        if (addr >= LOW_ALIAS_BASE && addr < LOW_ALIAS_BASE + mem_size) {
            return reinterpret_cast<char*>(mem_ptr + (addr - LOW_ALIAS_BASE));
        }
        return nullptr;
    }

    bool mmio_load(reg_t addr, size_t len, uint8_t* bytes) override {
        // Handle UART
        if (addr >= UART_BASE && addr < UART_BASE + UART_SIZE) {
            uint64_t value = uart_mmio_load(uart_ptr, addr, len);
            memcpy(bytes, &value, len);
            return true;
        }
        return false;
    }

    bool mmio_store(reg_t addr, size_t len, const uint8_t* bytes) override {
        // Handle exit address
        if (addr == SIM_EXIT_ADDR) {
            uint64_t value = 0;
            memcpy(&value, bytes, len);
            exit_code = static_cast<int>(value & 0xffffffff);
            exit_requested = true;
            return true;
        }

        // Handle UART
        if (addr >= UART_BASE && addr < UART_BASE + UART_SIZE) {
            uint64_t value = 0;
            memcpy(&value, bytes, len);
            return uart_mmio_store(uart_ptr, addr, len, value);
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
    int exit_code = 0;

private:
    uint8_t* mem_ptr;
    size_t mem_size;
    uint8_t* uart_ptr;
    std::string isa_storage;
    cfg_t cfg;
};

extern "C" {

int spike_run_raw(
    const char* isa,
    size_t,
    uint8_t* mem_ptr,
    size_t mem_size,
    uint64_t entry,
    uint64_t dtb_addr,
    const char* log_path,
    const uint64_t* tp_value_ptr,
    uint8_t* uart_ptr,
    bool pk
) {
    std::signal(SIGABRT, crash_handler);
    std::signal(SIGSEGV, crash_handler);
    std::signal(SIGILL, crash_handler);

    // Ensure buckyball extension is registered before creating processor
    ensure_buckyball_registered();

    // Create simif with Rust-provided memory and UART
    simple_simif_t simif(mem_ptr, mem_size, uart_ptr, isa);

    // Create processor
    FILE* log_file = nullptr;
    if (log_path && strlen(log_path) > 0) {
        log_file = fopen(log_path, "w");
        if (log_file == nullptr) {
            fprintf(stderr, "[ERROR] Failed to open log file: %s\n", log_path);
            fflush(stderr);
            return 1;
        }
    } else {
        fprintf(stderr, "[ERROR] log_path is required: bemu enables Spike debug mode and must write disasm.log\n");
        fprintf(stderr, "[ERROR] Pass --log-dir=<dir> when invoking bemu (e.g. --log-dir=/tmp/bemu_log)\n");
        fflush(stderr);
        return 1;
    }

    const char* final_isa = simif.get_cfg().isa;
    processor_t proc(final_isa, "MSU", &simif.get_cfg(), &simif, 0, false, log_file, std::cerr);

    // Reset processor to ensure clean state
    proc.reset();

    extension_t* mounted_ext = proc.get_extension("buckyball");
    if (mounted_ext == nullptr) {
        fprintf(stderr, "[ERROR] buckyball extension not mounted\n");
        fflush(stderr);
        return 1;
    }

    // Enable debug mode to force trap exceptions
    proc.set_debug(true);

    // Skip find_extension() as it tries to dlopen() and hangs
    // We've already registered the extension via ensure_buckyball_registered()

    // Set up initial state
    state_t* state = proc.get_state();

    const uint64_t TRAP_HANDLER_ADDR = 0x80000000 + mem_size - 0x2000;  // Trap handler
    const uint64_t SYSCALL_MAGIC_ADDR_INIT = 0x80000000 + mem_size - 0x1000; // Magic address for syscall detection

    if (pk) {
        // Linux mode: start in S-mode with syscall trap handler
        // IMPORTANT: Implement a real trap handler in memory
        // The trap handler will save the syscall number and arguments, then jump to a special address
        // We'll detect this special address and handle the syscall

        // Write a simple trap handler that jumps to our magic address
        // We'll use RISC-V assembly instructions encoded as uint32_t
        if (TRAP_HANDLER_ADDR >= DRAM_BASE && TRAP_HANDLER_ADDR + 64 <= DRAM_BASE + mem_size) {
            uint32_t* handler = reinterpret_cast<uint32_t*>(mem_ptr + (TRAP_HANDLER_ADDR - DRAM_BASE));

            // Trap handler code (RISC-V assembly):
            // We want to jump to SYSCALL_MAGIC_ADDR_INIT
            // Use: lui t0, %hi(addr); jalr zero, t0, %lo(addr)

            // Calculate hi and lo parts for lui/jalr
            // For a 64-bit address, we need to use multiple instructions
            // Let's use a simpler approach: just use a relative jump

            // Calculate offset from trap handler to magic address
            int64_t offset = SYSCALL_MAGIC_ADDR_INIT - TRAP_HANDLER_ADDR;

            // Use jal (jump and link) instruction: jal x0, offset
            // jal encoding: imm[20|10:1|11|19:12] | rd | opcode
            // opcode for jal = 0x6f
            // rd = 0 (x0, discard return address)

            // Split offset into immediate fields
            uint32_t imm20 = (offset >> 20) & 0x1;
            uint32_t imm10_1 = (offset >> 1) & 0x3ff;
            uint32_t imm11 = (offset >> 11) & 0x1;
            uint32_t imm19_12 = (offset >> 12) & 0xff;

            uint32_t jal_insn = 0x6f | // opcode
                               (0 << 7) | // rd = x0
                               (imm19_12 << 12) |
                               (imm11 << 20) |
                               (imm10_1 << 21) |
                               (imm20 << 31);

            handler[0] = jal_insn;

        }

        // Set mtvec to point to our trap handler
        // IMPORTANT: Supervisor mode ecalls trap to Machine mode, so we need to set mtvec, not stvec!
        state->csrmap[CSR_MTVEC]->write(TRAP_HANDLER_ADDR);

        // Start in Supervisor mode
        state->prv = PRV_S;

        // Enable floating point
        constexpr reg_t MSTATUS_FS_MASK = 0x6000;
        constexpr reg_t MSTATUS_FS_DIRTY = 0x6000;
        reg_t mstatus = state->csrmap[CSR_MSTATUS]->read();
        mstatus = (mstatus & ~MSTATUS_FS_MASK) | MSTATUS_FS_DIRTY;
        state->csrmap[CSR_MSTATUS]->write(mstatus);

        // Enable performance counters (cycle, time, instret) for S/U modes.
        // Real Linux kernels set these CSRs so user programs can read cycle counters.
        // Without this, `rdcycle` / `csrr a5, cycle` triggers illegal instruction trap.
        // mcounteren: M-mode allows S-mode to access counters
        // scounteren: S-mode allows U-mode to access counters
        // Bits: 0=cycle, 1=time, 2=instret, 3-31=hpmcounter3-31
        constexpr reg_t COUNTEREN_CY = 0x1;  // cycle
        constexpr reg_t COUNTEREN_TM = 0x2;  // time
        constexpr reg_t COUNTEREN_IR = 0x4;  // instret
        constexpr reg_t COUNTEREN_MASK = COUNTEREN_CY | COUNTEREN_TM | COUNTEREN_IR;
        state->csrmap[CSR_MCOUNTEREN]->write(COUNTEREN_MASK);
        state->csrmap[CSR_SCOUNTEREN]->write(COUNTEREN_MASK);

        state->pc = entry;
        state->XPR.write(10, 0);        // a0 = hartid (0)
        state->XPR.write(11, dtb_addr); // a1 = dtb address

        // Initialize Linux process stack (argc/argv/envp/auxv)
    // Layout:
    //   sp[0] = argc
    //   sp[1] = argv[0]
    //   sp[2] = NULL
    //   sp[3] = NULL (envp terminator)
    //   sp[4..] = auxv pairs, terminated by AT_NULL
    uint64_t stack_top = (DRAM_BASE + mem_size - 0x400000) & ~0xFULL;
    const char prog_name[] = "tutorial-linux";
    constexpr uint64_t AT_NULL = 0;
    constexpr uint64_t AT_PHDR = 3;
    constexpr uint64_t AT_PHENT = 4;
    constexpr uint64_t AT_PHNUM = 5;
    constexpr uint64_t AT_PAGESZ = 6;
    constexpr uint64_t AT_BASE = 7;
    constexpr uint64_t AT_HWCAP = 16;
    constexpr uint64_t AT_ENTRY = 9;
    constexpr uint64_t AT_UID = 11;
    constexpr uint64_t AT_EUID = 12;
    constexpr uint64_t AT_GID = 13;
    constexpr uint64_t AT_EGID = 14;
    constexpr uint64_t AT_HWCAP2 = 26;
    constexpr uint64_t AT_SECURE = 23;
    constexpr uint64_t AT_RANDOM = 25;
    constexpr uint64_t AT_EXECFN = 31;
    const uint64_t word_size = sizeof(uint64_t);
    const uint64_t stack_words = 36;
    const uint64_t string_len = sizeof(prog_name);
    const uint64_t random_len = 16;

    uint64_t string_addr = (stack_top - string_len) & ~0xFULL;
    uint64_t random_addr = (string_addr - random_len) & ~0xFULL;
    uint64_t sp = (random_addr - stack_words * word_size) & ~0xFULL;
    uint64_t at_phdr = 0;
    uint64_t at_phent = 56;
    uint64_t at_phnum = 0;

    // Read ELF header fields from guest memory for accurate auxv.
    if (mem_size >= 64) {
        uint64_t e_phoff = 0;
        uint16_t e_phentsize = 56;
        uint16_t e_phnum16 = 0;
        memcpy(&e_phoff, mem_ptr + 32, sizeof(uint64_t));
        memcpy(&e_phentsize, mem_ptr + 54, sizeof(uint16_t));
        memcpy(&e_phnum16, mem_ptr + 56, sizeof(uint16_t));
        if (e_phoff < mem_size) {
            at_phdr = DRAM_BASE + e_phoff;
        }
        if (e_phentsize != 0) {
            at_phent = e_phentsize;
        }
        at_phnum = e_phnum16;
    }

    if (sp < DRAM_BASE || stack_top > DRAM_BASE + mem_size) {
        fprintf(stderr, "[ERROR] Invalid stack range: sp=0x%lx top=0x%lx\n", sp, stack_top);
        fflush(stderr);
        return 1;
    }

    uint64_t sp_offset = sp - DRAM_BASE;
    uint64_t string_offset = string_addr - DRAM_BASE;
    uint64_t random_offset = random_addr - DRAM_BASE;
    if (string_offset + string_len > mem_size ||
        random_offset + random_len > mem_size ||
        sp_offset + stack_words * word_size > mem_size) {
        fprintf(stderr, "[ERROR] Stack layout exceeds guest memory\n");
        fflush(stderr);
        return 1;
    }

    memcpy(mem_ptr + string_offset, prog_name, string_len);
    for (uint64_t i = 0; i < random_len; ++i) {
        mem_ptr[random_offset + i] = static_cast<uint8_t>(0xA5u ^ static_cast<uint8_t>(i));
    }
    uint64_t* stack_words_ptr = reinterpret_cast<uint64_t*>(mem_ptr + sp_offset);
    stack_words_ptr[0] = 1;
    stack_words_ptr[1] = string_addr;
    stack_words_ptr[2] = 0;
    stack_words_ptr[3] = 0;
    stack_words_ptr[4] = AT_PHDR;
    stack_words_ptr[5] = at_phdr;
    stack_words_ptr[6] = AT_PHENT;
    stack_words_ptr[7] = at_phent;
    stack_words_ptr[8] = AT_PHNUM;
    stack_words_ptr[9] = at_phnum;
    stack_words_ptr[10] = AT_PAGESZ;
    stack_words_ptr[11] = 4096;
    stack_words_ptr[12] = AT_BASE;
    stack_words_ptr[13] = 0;
    stack_words_ptr[14] = AT_HWCAP;
    stack_words_ptr[15] = 0;
    stack_words_ptr[16] = AT_ENTRY;
    stack_words_ptr[17] = entry;
    stack_words_ptr[18] = AT_UID;
    stack_words_ptr[19] = 0;
    stack_words_ptr[20] = AT_EUID;
    stack_words_ptr[21] = 0;
    stack_words_ptr[22] = AT_GID;
    stack_words_ptr[23] = 0;
    stack_words_ptr[24] = AT_EGID;
    stack_words_ptr[25] = 0;
    stack_words_ptr[26] = AT_SECURE;
    stack_words_ptr[27] = 0;
    stack_words_ptr[28] = AT_RANDOM;
    stack_words_ptr[29] = random_addr;
    stack_words_ptr[30] = AT_HWCAP2;
    stack_words_ptr[31] = 0;
    stack_words_ptr[32] = AT_EXECFN;
    stack_words_ptr[33] = string_addr;
    stack_words_ptr[34] = AT_NULL;
    stack_words_ptr[35] = 0;

    state->XPR.write(2, sp); // sp = x2
    state->XPR.write(10, 1); // a0 = argc
    state->XPR.write(11, sp + word_size); // a1 = argv
    state->XPR.write(12, sp + 3 * word_size); // a2 = envp

    // Initialize tp (thread pointer) for TLS support if provided
    if (tp_value_ptr != nullptr) {
        uint64_t tp = *tp_value_ptr;
        state->XPR.write(4, tp); // tp = x4
    }
    } else {
        // Baremetal mode: enable FP, set PC, pass hartid/dtb. No stack setup.
        constexpr reg_t MSTATUS_FS_MASK = 0x6000;
        constexpr reg_t MSTATUS_FS_DIRTY = 0x6000;
        reg_t mstatus = state->csrmap[CSR_MSTATUS]->read();
        mstatus = (mstatus & ~MSTATUS_FS_MASK) | MSTATUS_FS_DIRTY;
        state->csrmap[CSR_MSTATUS]->write(mstatus);

        // Enable performance counters (cycle, time, instret) for S/U modes
        constexpr reg_t COUNTEREN_MASK = 0x7;  // cycle | time | instret
        state->csrmap[CSR_MCOUNTEREN]->write(COUNTEREN_MASK);
        state->csrmap[CSR_SCOUNTEREN]->write(COUNTEREN_MASK);

        state->pc = entry;
        state->XPR.write(10, 0);        // a0 = hartid (0)
        state->XPR.write(11, dtb_addr); // a1 = dtb address
    }

    // Run until sim_exit or syscall exit
    uint64_t step_count = 0;
    reg_t prev_pc = state->pc;

    const uint64_t SYSCALL_MAGIC_ADDR = 0x80000000 + mem_size - 0x1000;
    uint64_t pctrace_interval = 0;
    if (const char* pctrace_env = std::getenv("BEMU_PCTRACE")) {
        pctrace_interval = std::strtoull(pctrace_env, nullptr, 0);
        if (pctrace_interval <= 1) {
            pctrace_interval = 1000000;
        }
    }

    while (!simif.exit_requested && !should_exit()) {
        try {
            // Check if we're at the magic trap target
            if (state->pc == SYSCALL_MAGIC_ADDR) {
                reg_t mcause = state->csrmap[CSR_MCAUSE]->read();
                bool is_ecall =
                    mcause == CAUSE_USER_ECALL ||
                    mcause == CAUSE_SUPERVISOR_ECALL ||
                    mcause == CAUSE_MACHINE_ECALL;
                if (!is_ecall) {
                    reg_t mepc = state->csrmap[CSR_MEPC]->read();
                    fprintf(stderr,
                            "[ERROR] Non-ecall trap reached syscall handler: mcause=%ld mepc=0x%lx tval=0x%lx pc=0x%lx\n",
                            mcause, mepc, state->csrmap[CSR_MTVAL]->read(), state->pc);
                    fflush(stderr);
                    return 1;
                }

                // System call detected from ecall
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

                // Return to the instruction after ecall
                // IMPORTANT: Supervisor mode ecall traps to Machine mode, so use mepc not sepc
                reg_t epc = state->csrmap[CSR_MEPC]->read();
                state->pc = epc + 4;

                // Also need to restore privilege mode back to Supervisor
                state->prv = PRV_S;

                prev_pc = state->pc;
                continue;  // Skip the normal step
            }

            uint16_t insn16 = 0;
            bool has_insn16 = false;
            if (state->pc >= DRAM_BASE && state->pc + 2 <= DRAM_BASE + mem_size) {
                uint64_t off = state->pc - DRAM_BASE;
                memcpy(&insn16, mem_ptr + off, sizeof(insn16));
                has_insn16 = true;
            } else if (state->pc >= LOW_ALIAS_BASE && state->pc + 2 <= LOW_ALIAS_BASE + mem_size) {
                uint64_t off = state->pc - LOW_ALIAS_BASE;
                memcpy(&insn16, mem_ptr + off, sizeof(insn16));
                has_insn16 = true;
            }
            if (has_insn16 && (insn16 & 0xF07F) == 0x9002) {
                uint32_t rs1 = (insn16 >> 7) & 0x1F;
                reg_t target = state->XPR[rs1] & ~((reg_t)1);
                bool valid_high = target >= DRAM_BASE && target < DRAM_BASE + mem_size;
                bool valid_low_alias = target >= LOW_ALIAS_BASE && target < LOW_ALIAS_BASE + mem_size;
                if (!valid_high && !valid_low_alias) {
                    state->pc += 2;
                    prev_pc = state->pc;
                    continue;
                }
            }

            proc.step(1);
            step_count++;
            if (pctrace_interval != 0 && step_count % pctrace_interval == 0) {
                fprintf(stderr,
                        "[PCTRACE] step=%lu pc=0x%lx sp=0x%lx ra=0x%lx a0=0x%lx a1=0x%lx\n",
                        step_count, state->pc, state->XPR[2], state->XPR[1], state->XPR[10], state->XPR[11]);
                fflush(stderr);
            }

            // Detect PC jump to 0
            if (state->pc == 0 && prev_pc != 0) {
                fprintf(stderr, "[ERROR] PC jumped to 0! Previous PC = 0x%lx, step = %lu\n", prev_pc, step_count);
                fprintf(stderr, "[ERROR] Register dump:\n");
                fprintf(stderr, "  ra (x1)  = 0x%lx\n", state->XPR[1]);
                fprintf(stderr, "  sp (x2)  = 0x%lx\n", state->XPR[2]);
                fprintf(stderr, "  gp (x3)  = 0x%lx\n", state->XPR[3]);
                fprintf(stderr, "  tp (x4)  = 0x%lx\n", state->XPR[4]);
                fprintf(stderr, "  t0 (x5)  = 0x%lx\n", state->XPR[5]);
                fprintf(stderr, "  t1 (x6)  = 0x%lx\n", state->XPR[6]);
                fprintf(stderr, "  t2 (x7)  = 0x%lx\n", state->XPR[7]);
                fprintf(stderr, "  s1 (x9)  = 0x%lx\n", state->XPR[9]);
                fprintf(stderr, "  a0 (x10) = 0x%lx\n", state->XPR[10]);
                fprintf(stderr, "  a1 (x11) = 0x%lx\n", state->XPR[11]);
                fprintf(stderr, "  a7 (x17) = 0x%lx\n", state->XPR[17]);
                fprintf(stderr, "  t4 (x29) = 0x%lx\n", state->XPR[29]);

                // Read instruction at previous PC
                if (prev_pc >= DRAM_BASE && prev_pc < DRAM_BASE + mem_size) {
                    uint64_t offset = prev_pc - DRAM_BASE;
                    uint32_t insn = *(uint32_t*)(mem_ptr + offset);
                    fprintf(stderr, "  Instruction at 0x%lx: 0x%08x\n", prev_pc, insn);
                }
                fflush(stderr);
                // Exit immediately to avoid infinite loop
                break;
            }

            prev_pc = state->pc;
        } catch (trap_t& t) {
            // Handle traps (exceptions)

            if (t.cause() == CAUSE_USER_ECALL ||
                t.cause() == CAUSE_SUPERVISOR_ECALL ||
                t.cause() == CAUSE_MACHINE_ECALL) {
                // System call - get arguments from registers
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

                // IMPORTANT: For ecalls, we need to return to the instruction AFTER the ecall
                // The PC in mepc/sepc points to the ecall instruction itself
                // We need to read mepc, add 4, and set PC to that value
                reg_t epc;
                if (t.cause() == CAUSE_MACHINE_ECALL) {
                    epc = state->csrmap[CSR_MEPC]->read();
                } else if (t.cause() == CAUSE_SUPERVISOR_ECALL) {
                    epc = state->csrmap[CSR_SEPC]->read();
                } else {
                    epc = state->pc;  // User ecall - PC should already be set correctly
                }

                // Return to the instruction after ecall
                state->pc = epc + 4;

            } else if (t.cause() == CAUSE_BREAKPOINT) {
                // Breakpoint (ebreak) - this might be our trap handler
                // Skip the ebreak instruction
                state->pc += 4;
            } else if (t.cause() == CAUSE_MISALIGNED_LOAD || t.cause() == CAUSE_MISALIGNED_STORE) {
                // Misaligned load/store: Spike's MMU will handle this in software.
                // The trap handler has already been invoked by Spike internally,
                // and the PC has been updated. We just need to continue execution.
                // Do nothing - let Spike's internal misaligned handler take care of it.
            } else {
                reg_t mcause = state->csrmap[CSR_MCAUSE]->read();
                reg_t mepc = state->csrmap[CSR_MEPC]->read();
                reg_t tval = state->csrmap[CSR_MTVAL]->read();
                fprintf(stderr,
                        "[ERROR] Unhandled trap: cause=%ld mcause=%ld mepc=0x%lx tval=0x%lx pc=0x%lx\n",
                        t.cause(), mcause, mepc, tval, state->pc);
                fflush(stderr);
                return 1;
            }
        }
    }

    if (log_file) {
        fclose(log_file);
    }

    if (simif.exit_requested) {
        return simif.exit_code;
    }
    return should_exit() ? get_exit_code_ffi() : 0;
}
}
