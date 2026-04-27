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

REGISTER_EXTENSION(buckyball, []() {
  static buckyball_rocc_t ext;
  return &ext;
})
