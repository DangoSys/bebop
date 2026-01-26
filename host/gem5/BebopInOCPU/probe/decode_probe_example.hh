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
 * Example of using SignalCollector to probe decode stage uop information.
 * This demonstrates how to set up read-only signal collection from the
 * decode stage without modifying the decode implementation.
 */

#ifndef __CPU_BEBOPINO_PROBE_DECODE_PROBE_EXAMPLE_HH__
#define __CPU_BEBOPINO_PROBE_DECODE_PROBE_EXAMPLE_HH__

#include "signal_collector.hh"
#include "cpu/bebopino/decode.hh"
#include "cpu/bebopino/pipeline.hh"

namespace gem5
{

namespace bbino
{

/**
 * DecodeProbe - Example class showing how to use SignalCollector
 * to monitor decode stage uop information
 */
class DecodeProbe
{
  private:
    /** Reference to the decode stage being monitored */
    const Decode &decode;

    /** Reference to the pipeline */
    const Pipeline &pipeline;

    /** Signal collector instance */
    SignalCollector collector;

  public:
    /**
     * Constructor
     * @param decode_ Reference to decode stage
     * @param pipeline_ Reference to pipeline
     */
    DecodeProbe(const Decode &decode_, const Pipeline &pipeline_);

    /**
     * Initialize signal probes
     * This registers all the signals we want to collect
     */
    void setupProbes();

    /**
     * Collect signals for current cycle
     * Call this each cycle to sample all registered signals
     */
    void collect() { collector.collect(); }

    /**
     * Enable/disable collection
     */
    void setEnabled(bool enable) { collector.setEnabled(enable); }

    /**
     * Enable trace file output
     */
    bool enableTrace(const std::string &file_path) {
        return collector.enableTrace(file_path);
    }

    /**
     * Get the signal collector
     */
    SignalCollector& getCollector() { return collector; }
    const SignalCollector& getCollector() const { return collector; }
};

} // namespace bbino
} // namespace gem5

#endif // __CPU_BEBOPINO_PROBE_DECODE_PROBE_EXAMPLE_HH__
