/*
 * Copyright (c) 2024 ARM Limited
 * All rights reserved
 *
 * The license below extends only to copyright in the software and shall
 * not be construed as granting a license to any other intellectual
 * property including but not limited to intellectual property relating
 * to a hardware implementation of the functionality of the software
 * licensed hereunder.  You may use the software subject to the license
 * terms below provided that you ensure that this notice is replicated
 * unmodified and in its entirety in all distributions of the software,
 * modified or unmodified, in source code or in binary form.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions are
 * met: redistributions of source code must retain the above copyright
 * notice, this list of conditions and the following disclaimer;
 * redistributions in binary form must reproduce the above copyright
 * notice, this list of conditions and the following disclaimer in the
 * documentation and/or other materials provided with the distribution;
 * neither the name of the copyright holders nor the names of its
 * contributors may be used to endorse or promote products derived from
 * this software without specific prior written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
 * "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
 * LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
 * A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
 * OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
 * SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
 * LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
 * DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
 * THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
 * (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
 * OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
 */

/**
 * @file
 *
 * Generic signal collector for BebopInOCPU monitoring.
 * This class provides a flexible framework for collecting arbitrary signals
 * from the CPU in a read-only manner.
 */

#ifndef __CPU_BEBOPINO_PROBE_SIGNAL_COLLECTOR_HH__
#define __CPU_BEBOPINO_PROBE_SIGNAL_COLLECTOR_HH__

#include <vector>
#include <string>
#include <map>
#include <memory>
#include <functional>
#include <fstream>

#include "base/named.hh"
#include "base/types.hh"

namespace gem5
{

namespace bbino
{

/**
 * Signal value types that can be collected
 */
enum class SignalType
{
    BOOL,
    UINT8,
    UINT16,
    UINT32,
    UINT64,
    INT8,
    INT16,
    INT32,
    INT64,
    ADDR,
    TICK,
    DOUBLE
};

/**
 * Generic signal value container
 */
struct SignalValue
{
    SignalType type;

    union {
        bool bool_val;
        uint8_t uint8_val;
        uint16_t uint16_val;
        uint32_t uint32_val;
        uint64_t uint64_val;
        int8_t int8_val;
        int16_t int16_val;
        int32_t int32_val;
        int64_t int64_val;
        Addr addr_val;
        Tick tick_val;
        double double_val;
    } data;

    SignalValue() : type(SignalType::UINT64) { data.uint64_val = 0; }

    explicit SignalValue(bool val) : type(SignalType::BOOL)
    { data.bool_val = val; }

    explicit SignalValue(uint8_t val) : type(SignalType::UINT8)
    { data.uint8_val = val; }

    explicit SignalValue(uint16_t val) : type(SignalType::UINT16)
    { data.uint16_val = val; }

    explicit SignalValue(uint32_t val) : type(SignalType::UINT32)
    { data.uint32_val = val; }

    explicit SignalValue(uint64_t val) : type(SignalType::UINT64)
    { data.uint64_val = val; }

    explicit SignalValue(int8_t val) : type(SignalType::INT8)
    { data.int8_val = val; }

    explicit SignalValue(int16_t val) : type(SignalType::INT16)
    { data.int16_val = val; }

    explicit SignalValue(int32_t val) : type(SignalType::INT32)
    { data.int32_val = val; }

    explicit SignalValue(int64_t val) : type(SignalType::INT64)
    { data.int64_val = val; }

    explicit SignalValue(Addr val) : type(SignalType::ADDR)
    { data.addr_val = val; }

    explicit SignalValue(Tick val) : type(SignalType::TICK)
    { data.tick_val = val; }

    explicit SignalValue(double val) : type(SignalType::DOUBLE)
    { data.double_val = val; }

    std::string toString() const;
    uint64_t toUint64() const;
};

/**
 * Signal probe - function that reads a signal value
 */
using SignalProbe = std::function<SignalValue()>;

/**
 * Signal snapshot - collection of signal values at a specific time
 */
struct SignalSnapshot
{
    Tick tick;
    uint64_t cycle;
    std::map<std::string, SignalValue> signals;

    SignalSnapshot() : tick(0), cycle(0) {}
};

/**
 * SignalCollector - Generic read-only signal collection framework
 *
 * This class provides a flexible way to register and collect arbitrary signals
 * from the BebopInOCPU without modifying the monitored components.
 *
 * Usage:
 *   1. Create a SignalCollector instance
 *   2. Register signal probes using registerSignal()
 *   3. Call collect() each cycle to sample all registered signals
 *   4. Query collected data via getLatestSnapshot() or getSnapshot()
 */
class SignalCollector : public Named
{
  private:
    /** Enable signal collection */
    bool enabled;

    /** Enable trace file output */
    bool trace_enabled;

    /** Trace file path */
    std::string trace_file_path;

    /** Trace file stream */
    std::ofstream trace_file;

    /** Registered signal probes */
    std::map<std::string, SignalProbe> signal_probes;

    /** Signal metadata (type, description) */
    std::map<std::string, std::pair<SignalType, std::string>> signal_metadata;

    /** History of signal snapshots */
    std::vector<SignalSnapshot> signal_history;

    /** Maximum history size (0 = unlimited) */
    size_t max_history_size;

    /** Current cycle count */
    uint64_t current_cycle;

  public:
    /**
     * Constructor
     * @param name Name of this signal collector
     */
    SignalCollector(const std::string &name);

    /** Destructor - closes trace file if open */
    ~SignalCollector();

    /**
     * Register a signal probe
     * @param signal_name Unique name for this signal
     * @param probe Function that returns the signal value
     * @param type Signal data type
     * @param description Optional description of the signal
     * @return True if registration successful
     */
    bool registerSignal(const std::string &signal_name,
                       SignalProbe probe,
                       SignalType type,
                       const std::string &description = "");

    /**
     * Unregister a signal probe
     * @param signal_name Name of signal to unregister
     * @return True if signal was found and removed
     */
    bool unregisterSignal(const std::string &signal_name);

    /**
     * Check if a signal is registered
     * @param signal_name Name of signal to check
     * @return True if signal is registered
     */
    bool isSignalRegistered(const std::string &signal_name) const;

    /**
     * Get list of all registered signal names
     * @return Vector of signal names
     */
    std::vector<std::string> getRegisteredSignals() const;

    /**
     * Enable or disable signal collection
     * @param enable True to enable, false to disable
     */
    void setEnabled(bool enable) { enabled = enable; }

    /**
     * Check if signal collection is enabled
     * @return True if enabled
     */
    bool isEnabled() const { return enabled; }

    /**
     * Enable trace file output
     * @param file_path Path to trace file
     * @return True if file opened successfully
     */
    bool enableTrace(const std::string &file_path);

    /**
     * Disable trace file output
     */
    void disableTrace();

    /**
     * Set maximum history size
     * @param size Maximum number of snapshots to keep (0 = unlimited)
     */
    void setMaxHistorySize(size_t size) { max_history_size = size; }

    /**
     * Get maximum history size
     * @return Maximum history size
     */
    size_t getMaxHistorySize() const { return max_history_size; }

    /**
     * Collect all registered signals (call this each cycle)
     * This samples all registered signal probes and stores the snapshot
     */
    void collect();

    /**
     * Get the most recent signal snapshot
     * @return Reference to the latest snapshot
     */
    const SignalSnapshot& getLatestSnapshot() const;

    /**
     * Get signal snapshot at a specific index
     * @param index Index in history (0 = oldest)
     * @return Reference to snapshot at index
     */
    const SignalSnapshot& getSnapshot(size_t index) const;

    /**
     * Get number of snapshots in history
     * @return Number of snapshots
     */
    size_t getHistorySize() const { return signal_history.size(); }

    /**
     * Clear signal history
     */
    void clearHistory();

    /**
     * Get current cycle count
     * @return Current cycle
     */
    uint64_t getCurrentCycle() const { return current_cycle; }

    /**
     * Query a specific signal value from the latest snapshot
     * @param signal_name Name of the signal
     * @param value Output parameter for the signal value
     * @return True if signal found in latest snapshot
     */
    bool querySignal(const std::string &signal_name, SignalValue &value) const;

    /**
     * Dump signal metadata to output stream
     * @param os Output stream
     */
    void dumpSignalInfo(std::ostream &os) const;

    /**
     * Dump statistics to output stream
     * @param os Output stream
     */
    void dumpStats(std::ostream &os) const;

  private:
    /**
     * Write trace file header
     */
    void writeTraceHeader();

    /**
     * Write current snapshot to trace file
     */
    void writeTraceEntry(const SignalSnapshot &snapshot);
};

} // namespace bbino
} // namespace gem5

#endif // __CPU_BEBOPINO_PROBE_SIGNAL_COLLECTOR_HH__
