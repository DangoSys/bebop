/*
 * Bebop NPU Coprocessor Module
 * Handles custom RISC-V instructions for NPU operations
 */

#ifndef __CPU_BEBOPINO_COPROCESSOR_HH__
#define __CPU_BEBOPINO_COPROCESSOR_HH__

#include <queue>
#include <cstdint>
#include <string>

namespace gem5
{

// Forward declaration for BebopInOCPU (defined in gem5 namespace, not bbino)
class BebopInOCPU;

namespace bbino
{

// Forward declarations
class Execute;
class LSQ;

/** Bebop instruction packet */
struct BebopInst
{
    uint64_t inst_encoding;  // Full instruction encoding
    uint8_t func7;           // Function code (24-27)
    uint64_t rs1_val;        // Source register 1 value
    uint64_t rs2_val;        // Source register 2 value
    uint64_t issue_tick;     // Tick when instruction was issued

    BebopInst(uint64_t encoding, uint8_t f7, uint64_t rs1, uint64_t rs2, uint64_t tick)
        : inst_encoding(encoding), func7(f7), rs1_val(rs1), rs2_val(rs2),
          issue_tick(tick)
    {}
};

/** Bebop Coprocessor
 *
 * Simple coprocessor model that:
 * 1. Receives custom instructions (opcode 0x7B)
 * 2. Processes them after a fixed latency (10 cycles)
 * 3. Prints the instruction details
 * 4. Can access L2 cache and main memory through the memory system
 */
class BebopCoprocessor
{
  private:
    /** Name of this coprocessor */
    std::string name;

    /** Reference to the CPU for accessing clock and memory system */
    BebopInOCPU &cpu;

    /** Reference to Execute stage for memory access */
    Execute &execute;

    /** Instruction queue */
    std::queue<BebopInst> instQueue;

    /** Processing latency in cycles */
    static const int PROCESSING_LATENCY = 10;

    /** Create a memory read request packet
     * @param addr Physical address to read from
     * @param size Number of bytes to read
     * @param use_l2_only True if request should go to L2 cache only
     * @return Packet pointer for the request
     */
    void* createReadRequest(uint64_t addr, size_t size, bool use_l2_only = false);

    /** Create a memory write request packet
     * @param addr Physical address to write to
     * @param size Number of bytes to write
     * @param use_l2_only True if request should go to L2 cache only
     * @return Packet pointer for the request
     */
    void* createWriteRequest(uint64_t addr, size_t size, bool use_l2_only = false);

  public:
    BebopCoprocessor(const std::string &name_, BebopInOCPU &cpu_, Execute &execute_);

    ~BebopCoprocessor() = default;

    /** Submit a custom instruction to the coprocessor
     * @param inst_encoding Full instruction encoding
     * @param func7 Function code (identifies operation type)
     * @param rs1_val Value of rs1 register
     * @param rs2_val Value of rs2 register
     * @param current_tick Current simulation tick
     */
    void submitInstruction(uint64_t inst_encoding, uint8_t func7,
                          uint64_t rs1_val, uint64_t rs2_val,
                          uint64_t current_tick);

    /** Complete an instruction after processing (for simulation purposes) */
    void completeInstruction(const BebopInst &inst);

    /** Request to read data from memory
     * @param addr Physical address to read from
     * @param size Number of bytes to read
     * @param use_l2_only True to access L2 cache, False for main memory
     * @return True if request was successful
     */
    bool readMemory(uint64_t addr, size_t size, bool use_l2_only = false);

    /** Request to write data to memory
     * @param addr Physical address to write to
     * @param size Number of bytes to write
     * @param use_l2_only True to access L2 cache, False for main memory
     * @return True if request was successful
     */
    bool writeMemory(uint64_t addr, size_t size, bool use_l2_only = false);

    /** Get the processing latency in cycles */
    int getLatency() const { return PROCESSING_LATENCY; }

    /** Check if coprocessor is idle */
    bool isIdle() const { return instQueue.empty(); }

    /** Get the number of pending instructions */
    size_t getPendingCount() const { return instQueue.size(); }
};

} // namespace bbino
} // namespace gem5

#endif // __CPU_BEBOPINO_COPROCESSOR_HH__
