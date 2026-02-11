/*
 * Bebop NPU Coprocessor Module Implementation
 * Real memory access through gem5 memory system
 */

#include "bebop/coprocessor.hh"
#include "execute.hh"
#include "lsq.hh"
#include "cpu.hh"

#include "mem/request.hh"
#include "mem/packet.hh"
#include "sim/system.hh"

#include <iomanip>
#include <iostream>

namespace gem5
{

// Forward declarations to avoid circular dependency
class BebopInOCPU;

namespace bbino
{

// Forward declaration
class Execute;

BebopCoprocessor::BebopCoprocessor(const std::string &name_, BebopInOCPU &cpu_, Execute &execute_)
    : name(name_), cpu(cpu_), execute(execute_)
{
    std::cout << "BebopCoprocessor: Initialized '" << name
              << "' with real memory access capabilities\n";
}

void
BebopCoprocessor::submitInstruction(uint64_t inst_encoding, uint8_t func7,
                                    uint64_t rs1_val, uint64_t rs2_val,
                                    uint64_t current_tick)
{
    // Create instruction packet
    BebopInst inst(inst_encoding, func7, rs1_val, rs2_val, current_tick);

    // Add to queue
    instQueue.push(inst);

    std::cout << "BebopCoprocessor: Received instruction 0x" << std::hex
              << (uint32_t)inst_encoding << std::dec << " (func7=" << (int)func7
              << ") at tick " << current_tick << "\n";

    // For simplicity, we immediately process after 10 cycles
    // In a real implementation, this would be scheduled via events
    uint64_t completion_tick = current_tick + PROCESSING_LATENCY;

    std::cout << "BebopCoprocessor: Will complete at tick " << completion_tick
              << " (in " << PROCESSING_LATENCY << " cycles)\n";
}

void
BebopCoprocessor::completeInstruction(const BebopInst &inst)
{
    // Remove from queue
    if (!instQueue.empty() && instQueue.front().inst_encoding == inst.inst_encoding) {
        instQueue.pop();
    }

    uint64_t current_tick = inst.issue_tick + PROCESSING_LATENCY;
    uint64_t elapsed = current_tick - inst.issue_tick;

    // Print instruction completion information
    std::cout << "\n========== BEBOP COPROCESSOR ==========\n";
    std::cout << "Instruction completed after " << elapsed << " ticks ("
              << PROCESSING_LATENCY << " cycles)\n";
    std::cout << "  Encoding:   0x" << std::hex << std::setw(8) << std::setfill('0')
              << (uint32_t)inst.inst_encoding << std::dec << "\n";
    std::cout << "  Function:   func7=" << (int)inst.func7;

    // Decode function type
    switch (inst.func7) {
        case 24:
            std::cout << " (BB_MVIN - Move data to NPU buffer)\n";
            std::cout << "    mem_addr: 0x" << std::hex << (uint32_t)inst.rs1_val << std::dec << "\n";
            std::cout << "    config:   0x" << std::hex << inst.rs2_val << std::dec << "\n";
            {
                uint32_t bank_id = inst.rs2_val & 0x1F;
                uint32_t depth = (inst.rs2_val >> 5) & 0x3FF;
                uint32_t stride = (inst.rs2_val >> 15) & 0x7FFFF;
                std::cout << "      bank_id=" << bank_id << ", depth=" << depth
                         << ", stride=" << stride << "\n";
            }
            break;
        case 25:
            std::cout << " (BB_MVOUT - Move data from NPU buffer)\n";
            std::cout << "    mem_addr: 0x" << std::hex << (uint32_t)inst.rs1_val << std::dec << "\n";
            std::cout << "    config:   0x" << std::hex << inst.rs2_val << std::dec << "\n";
            {
                uint32_t bank_id = inst.rs2_val & 0x1F;
                uint32_t depth = (inst.rs2_val >> 5) & 0x3FF;
                uint32_t stride = (inst.rs2_val >> 15) & 0x7FFFF;
                std::cout << "      bank_id=" << bank_id << ", depth=" << depth
                         << ", stride=" << stride << "\n";
            }
            break;
        case 26:
            std::cout << " (BB_MGATHER - Gather load)\n";
            std::cout << "    base+vlen: 0x" << std::hex << inst.rs1_val << std::dec << "\n";
            std::cout << "    offsets:   0x" << std::hex << inst.rs2_val << std::dec << "\n";
            break;
        case 27:
            std::cout << " (BB_GEMM - Matrix multiply)\n";
            std::cout << "    operands: 0x" << std::hex << inst.rs1_val << std::dec << "\n";
            std::cout << "    output:   0x" << std::hex << inst.rs2_val << std::dec << "\n";
            {
                uint32_t op1_addr = inst.rs1_val & 0xFF;
                uint32_t op2_addr = (inst.rs1_val >> 8) & 0xFF;
                uint32_t op3_addr = inst.rs2_val & 0xFF;
                std::cout << "      op1_addr=" << op1_addr << ", op2_addr=" << op2_addr
                         << ", op3_addr=" << op3_addr << "\n";
            }
            break;
        default:
            std::cout << " (UNKNOWN)\n";
            std::cout << "    rs1: 0x" << std::hex << inst.rs1_val << std::dec << "\n";
            std::cout << "    rs2: 0x" << std::hex << inst.rs2_val << std::dec << "\n";
            break;
    }

    std::cout << "  Issue tick: " << inst.issue_tick << "\n";
    std::cout << "  Done tick:  " << current_tick << "\n";

    // Perform memory operations based on instruction type
    switch (inst.func7) {
        case 24:  // BB_MVIN - Read from main memory
            {
                uint32_t mem_addr = (uint32_t)inst.rs1_val;
                uint32_t depth = (inst.rs2_val >> 5) & 0x3FF;
                uint32_t stride = (inst.rs2_val >> 15) & 0x7FFFF;
                size_t total_size = depth * stride;
                std::cout << "  Memory Access: Reading " << total_size
                         << " bytes from MAIN MEMORY at 0x" << std::hex << mem_addr << std::dec << "\n";
                // Access main memory (goes through L1 -> L2 -> Main Memory)
                readMemory(mem_addr, total_size, false);
            }
            break;
        case 25:  // BB_MVOUT - Write to main memory
            {
                uint32_t mem_addr = (uint32_t)inst.rs1_val;
                uint32_t depth = (inst.rs2_val >> 5) & 0x3FF;
                uint32_t stride = (inst.rs2_val >> 15) & 0x7FFFF;
                size_t total_size = depth * stride;
                std::cout << "  Memory Access: Writing " << total_size
                         << " bytes to MAIN MEMORY at 0x" << std::hex << mem_addr << std::dec << "\n";
                // Access main memory (goes through L1 -> L2 -> Main Memory)
                writeMemory(mem_addr, total_size, false);
            }
            break;
        case 26:  // BB_MGATHER - Read from L2 cache
            {
                uint64_t base_addr = inst.rs1_val & 0xFFFFFFFF;
                std::cout << "  Memory Access: Gather read from L2 CACHE at base 0x"
                         << std::hex << base_addr << std::dec << "\n";
                // Each gather reads 8 vectors, assume 64 bytes per vector
                // Access L2 cache directly (bypasses L1, goes L2 -> Main Memory if miss)
                readMemory(base_addr, 8 * 64, true);
            }
            break;
        case 27:  // BB_GEMM - No direct memory access in this phase
            std::cout << "  No memory access (compute only)\n";
            break;
    }

    std::cout << "=======================================\n" << std::endl;
}

bool
BebopCoprocessor::readMemory(uint64_t addr, size_t size, bool use_l2_only)
{
    const char* target = use_l2_only ? "L2 CACHE" : "MAIN MEMORY (via L1->L2)";
    std::cout << "    [Memory Read Request] Target: " << target
             << ", Address: 0x" << std::hex << addr << std::dec
             << ", Size: " << size << " bytes\n";

    // Create memory request
    RequestPtr req = std::make_shared<Request>(
        addr,                           // Physical address
        size,                          // Size in bytes
        0,                            // Flags
        cpu.dataRequestorId()         // Requestor ID
    );

    // Set request flags based on access type
    if (use_l2_only) {
        // For L2-only access (MGATHER), bypass L1 cache
        req->setFlags(Request::UNCACHEABLE);  // Force to bypass L1
        std::cout << "    [L2 Access] Bypassing L1 cache, direct to L2\n";
    } else {
        // Normal access through cache hierarchy
        std::cout << "    [Full Access] Through L1 -> L2 -> Main Memory\n";
    }

    // Create packet for read request
    PacketPtr pkt = new Packet(req, MemCmd::ReadReq);
    pkt->allocate();  // Allocate data buffer for response

    // Send request through dcache port via LSQ
    if (execute.getLSQ().getDcachePort().sendTimingReq(pkt)) {
        std::cout << "    [Memory Read] Request sent successfully\n";
        return true;
    } else {
        std::cout << "    [Memory Read] Request blocked, will retry\n";
        delete pkt;
        return false;
    }
}

bool
BebopCoprocessor::writeMemory(uint64_t addr, size_t size, bool use_l2_only)
{
    const char* target = use_l2_only ? "L2 CACHE" : "MAIN MEMORY (via L1->L2)";
    std::cout << "    [Memory Write Request] Target: " << target
             << ", Address: 0x" << std::hex << addr << std::dec
             << ", Size: " << size << " bytes\n";

    // Create memory request
    RequestPtr req = std::make_shared<Request>(
        addr,                           // Physical address
        size,                          // Size in bytes
        0,                            // Flags
        cpu.dataRequestorId()         // Requestor ID
    );

    // Set request flags based on access type
    if (use_l2_only) {
        // For L2-only access, bypass L1 cache
        req->setFlags(Request::UNCACHEABLE);
        std::cout << "    [L2 Access] Bypassing L1 cache, direct to L2\n";
    } else {
        // Normal access through cache hierarchy
        std::cout << "    [Full Access] Through L1 -> L2 -> Main Memory\n";
    }

    // Create packet for write request
    PacketPtr pkt = new Packet(req, MemCmd::WriteReq);
    pkt->allocate();  // Allocate data buffer

    // Fill with dummy data (in real implementation, copy from NPU buffer)
    uint8_t *data = pkt->getPtr<uint8_t>();
    for (size_t i = 0; i < size; i++) {
        data[i] = 0xBE;  // Bebop signature pattern
    }

    // Send request through dcache port via LSQ
    if (execute.getLSQ().getDcachePort().sendTimingReq(pkt)) {
        std::cout << "    [Memory Write] Request sent successfully\n";
        return true;
    } else {
        std::cout << "    [Memory Write] Request blocked, will retry\n";
        delete pkt;
        return false;
    }
}

} // namespace bbino
} // namespace gem5
