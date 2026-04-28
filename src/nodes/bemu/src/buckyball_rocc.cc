#include "rocc.h"

extern "C" {
  void buckyball_init();
  void buckyball_reset();
  uint64_t buckyball_exec(uint8_t funct7, uint64_t xs1, uint64_t xs2);
}

class buckyball_rocc_t : public rocc_t {
 public:
  const char* name() const override { return "buckyball"; }

  buckyball_rocc_t() {
    buckyball_init();
  }

  void reset(processor_t &) override {
    buckyball_reset();
  }

  reg_t custom0(processor_t *, rocc_insn_t insn, reg_t xs1, reg_t xs2) override {
    return buckyball_exec(insn.funct, xs1, xs2);
  }

  reg_t custom1(processor_t *, rocc_insn_t insn, reg_t xs1, reg_t xs2) override {
    return buckyball_exec(insn.funct, xs1, xs2);
  }

  reg_t custom2(processor_t *, rocc_insn_t insn, reg_t xs1, reg_t xs2) override {
    return buckyball_exec(insn.funct, xs1, xs2);
  }

  reg_t custom3(processor_t *, rocc_insn_t insn, reg_t xs1, reg_t xs2) override {
    return buckyball_exec(insn.funct, xs1, xs2);
  }
};

// Export the factory function so spike_wrapper.cc can call it directly
std::function<extension_t*()> buckyball_extension_factory = []() {
  static buckyball_rocc_t ext;
  return &ext;
};

// NOTE: Do NOT use REGISTER_EXTENSION macro here, as it will cause Spike to
// try to dlopen() the extension library, which doesn't exist. Instead, we
// manually register the extension in spike_wrapper.cc via ensure_buckyball_registered().
