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

#include "signal_collector.hh"

#include <iomanip>
#include <sstream>

#include "sim/core.hh"

namespace gem5
{

namespace bbino
{

// SignalValue implementation
std::string
SignalValue::toString() const
{
    std::ostringstream oss;
    switch (type) {
      case SignalType::BOOL:
        oss << (data.bool_val ? "true" : "false");
        break;
      case SignalType::UINT8:
        oss << static_cast<unsigned>(data.uint8_val);
        break;
      case SignalType::UINT16:
        oss << data.uint16_val;
        break;
      case SignalType::UINT32:
        oss << data.uint32_val;
        break;
      case SignalType::UINT64:
        oss << data.uint64_val;
        break;
      case SignalType::INT8:
        oss << static_cast<int>(data.int8_val);
        break;
      case SignalType::INT16:
        oss << data.int16_val;
        break;
      case SignalType::INT32:
        oss << data.int32_val;
        break;
      case SignalType::INT64:
        oss << data.int64_val;
        break;
      case SignalType::ADDR:
        oss << "0x" << std::hex << data.addr_val;
        break;
      case SignalType::TICK:
        oss << data.tick_val;
        break;
      case SignalType::DOUBLE:
        oss << data.double_val;
        break;
    }
    return oss.str();
}

uint64_t
SignalValue::toUint64() const
{
    switch (type) {
      case SignalType::BOOL:
        return data.bool_val ? 1 : 0;
      case SignalType::UINT8:
        return data.uint8_val;
      case SignalType::UINT16:
        return data.uint16_val;
      case SignalType::UINT32:
        return data.uint32_val;
      case SignalType::UINT64:
        return data.uint64_val;
      case SignalType::INT8:
        return static_cast<uint64_t>(data.int8_val);
      case SignalType::INT16:
        return static_cast<uint64_t>(data.int16_val);
      case SignalType::INT32:
        return static_cast<uint64_t>(data.int32_val);
      case SignalType::INT64:
        return static_cast<uint64_t>(data.int64_val);
      case SignalType::ADDR:
        return data.addr_val;
      case SignalType::TICK:
        return data.tick_val;
      case SignalType::DOUBLE:
        return static_cast<uint64_t>(data.double_val);
      default:
        return 0;
    }
}

// SignalCollector implementation
SignalCollector::SignalCollector(const std::string &name)
    : Named(name),
      enabled(false),
      trace_enabled(false),
      max_history_size(0),
      current_cycle(0)
{
}

SignalCollector::~SignalCollector()
{
    if (trace_file.is_open()) {
        trace_file.close();
    }
}

bool
SignalCollector::registerSignal(const std::string &signal_name,
                                SignalProbe probe,
                                SignalType type,
                                const std::string &description)
{
    if (signal_probes.find(signal_name) != signal_probes.end()) {
        return false;
    }

    signal_probes[signal_name] = probe;
    signal_metadata[signal_name] = std::make_pair(type, description);
    return true;
}

bool
SignalCollector::unregisterSignal(const std::string &signal_name)
{
    auto it = signal_probes.find(signal_name);
    if (it == signal_probes.end()) {
        return false;
    }

    signal_probes.erase(it);
    signal_metadata.erase(signal_name);
    return true;
}

bool
SignalCollector::isSignalRegistered(const std::string &signal_name) const
{
    return signal_probes.find(signal_name) != signal_probes.end();
}

std::vector<std::string>
SignalCollector::getRegisteredSignals() const
{
    std::vector<std::string> names;
    for (const auto &pair : signal_probes) {
        names.push_back(pair.first);
    }
    return names;
}

bool
SignalCollector::enableTrace(const std::string &file_path)
{
    if (trace_file.is_open()) {
        trace_file.close();
    }

    trace_file_path = file_path;
    trace_file.open(trace_file_path, std::ios::out | std::ios::trunc);

    if (!trace_file.is_open()) {
        return false;
    }

    trace_enabled = true;
    writeTraceHeader();
    return true;
}

void
SignalCollector::disableTrace()
{
    trace_enabled = false;
    if (trace_file.is_open()) {
        trace_file.close();
    }
}

void
SignalCollector::collect()
{
    if (!enabled) {
        return;
    }

    SignalSnapshot snapshot;
    snapshot.tick = curTick();
    snapshot.cycle = current_cycle++;

    // Sample all registered signal probes
    for (const auto &pair : signal_probes) {
        const std::string &name = pair.first;
        const SignalProbe &probe = pair.second;

        try {
            SignalValue value = probe();
            snapshot.signals[name] = value;
        } catch (...) {
            // If probe fails, skip this signal
        }
    }

    // Store snapshot in history
    signal_history.push_back(snapshot);

    // Enforce history size limit
    if (max_history_size > 0 && signal_history.size() > max_history_size) {
        signal_history.erase(signal_history.begin());
    }

    // Write to trace file if enabled
    if (trace_enabled) {
        writeTraceEntry(snapshot);
    }
}

void
SignalCollector::clearHistory()
{
    signal_history.clear();
}

const SignalSnapshot&
SignalCollector::getLatestSnapshot() const
{
    static SignalSnapshot empty_snapshot;
    if (signal_history.empty()) {
        return empty_snapshot;
    }
    return signal_history.back();
}

const SignalSnapshot&
SignalCollector::getSnapshot(size_t index) const
{
    static SignalSnapshot empty_snapshot;
    if (index >= signal_history.size()) {
        return empty_snapshot;
    }
    return signal_history[index];
}

bool
SignalCollector::querySignal(const std::string &signal_name,
                            SignalValue &value) const
{
    if (signal_history.empty()) {
        return false;
    }

    const SignalSnapshot &latest = signal_history.back();
    auto it = latest.signals.find(signal_name);
    if (it == latest.signals.end()) {
        return false;
    }

    value = it->second;
    return true;
}

void
SignalCollector::dumpSignalInfo(std::ostream &os) const
{
    os << "Signal Collector: " << Named::name() << std::endl;
    os << "Registered Signals: " << signal_probes.size() << std::endl;
    os << std::endl;

    for (const auto &pair : signal_metadata) {
        const std::string &sig_name = pair.first;
        const auto &metadata = pair.second;

        os << "  Signal: " << sig_name << std::endl;
        os << "    Type: ";

        switch (metadata.first) {
          case SignalType::BOOL: os << "bool"; break;
          case SignalType::UINT8: os << "uint8"; break;
          case SignalType::UINT16: os << "uint16"; break;
          case SignalType::UINT32: os << "uint32"; break;
          case SignalType::UINT64: os << "uint64"; break;
          case SignalType::INT8: os << "int8"; break;
          case SignalType::INT16: os << "int16"; break;
          case SignalType::INT32: os << "int32"; break;
          case SignalType::INT64: os << "int64"; break;
          case SignalType::ADDR: os << "Addr"; break;
          case SignalType::TICK: os << "Tick"; break;
          case SignalType::DOUBLE: os << "double"; break;
        }

        os << std::endl;
        if (!metadata.second.empty()) {
            os << "    Description: " << metadata.second << std::endl;
        }
        os << std::endl;
    }
}

void
SignalCollector::dumpStats(std::ostream &os) const
{
    os << "Signal Collector Statistics" << std::endl;
    os << "  Name: " << Named::name() << std::endl;
    os << "  Enabled: " << (enabled ? "Yes" : "No") << std::endl;
    os << "  Trace Enabled: " << (trace_enabled ? "Yes" : "No") << std::endl;
    os << "  Current Cycle: " << current_cycle << std::endl;
    os << "  History Size: " << signal_history.size() << std::endl;
    os << "  Max History Size: ";
    if (max_history_size == 0) {
        os << "Unlimited";
    } else {
        os << max_history_size;
    }
    os << std::endl;
    os << "  Registered Signals: " << signal_probes.size() << std::endl;
}

void
SignalCollector::writeTraceHeader()
{
    if (!trace_file.is_open()) {
        return;
    }

    trace_file << "# BebopInOCPU Signal Trace" << std::endl;
    trace_file << "# Collector: " << Named::name() << std::endl;
    trace_file << "# Columns: Tick, Cycle";

    // Add signal names to header
    for (const auto &pair : signal_probes) {
        trace_file << ", " << pair.first;
    }

    trace_file << std::endl;
}

void
SignalCollector::writeTraceEntry(const SignalSnapshot &snapshot)
{
    if (!trace_file.is_open()) {
        return;
    }

    trace_file << snapshot.tick << ", " << snapshot.cycle;

    // Write signal values in the same order as header
    for (const auto &pair : signal_probes) {
        const std::string &sig_name = pair.first;
        auto it = snapshot.signals.find(sig_name);

        if (it != snapshot.signals.end()) {
            trace_file << ", " << it->second.toString();
        } else {
            trace_file << ", N/A";
        }
    }

    trace_file << std::endl;
}

} // namespace bbino
} // namespace gem5
