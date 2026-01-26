/*
 * Copyright (c) 2024 ARM Limited
 * All rights reserved
 */

#include "decode_probe_example.hh"
#include "cpu/bebopino/pipe_data.hh"

namespace gem5
{

namespace bbino
{

DecodeProbe::DecodeProbe(const Decode &decode_, const Pipeline &pipeline_)
    : decode(decode_),
      pipeline(pipeline_),
      collector("decode_probe")
{
    setupProbes();
}

void
DecodeProbe::setupProbes()
{
    // 读取decode的inputBuffer占用数量（thread 0）
    collector.registerSignal(
        "decode_input_occupancy",
        [this]() {
            // 访问decode的public成员inputBuffer，读取其占用数量
            size_t occupancy = decode.inputBuffer[0].occupancy();
            return SignalValue(static_cast<uint64_t>(occupancy));
        },
        SignalType::UINT64,
        "Number of instructions in decode input buffer"
    );
}

} // namespace bbino
} // namespace gem5
