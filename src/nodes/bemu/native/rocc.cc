#include "rocc.h"
#include "processor.h"

extern "C" {
  void buckyball_init();
  void buckyball_reset();
  uint64_t buckyball_exec(uint8_t funct7, uint64_t xs1, uint64_t xs2, uint64_t pc);
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

  reg_t custom0(processor_t *p, rocc_insn_t insn, reg_t xs1, reg_t xs2) override {
    return buckyball_exec(insn.funct, xs1, xs2, p->get_state()->pc);
  }

  reg_t custom1(processor_t *p, rocc_insn_t insn, reg_t xs1, reg_t xs2) override {
    return buckyball_exec(insn.funct, xs1, xs2, p->get_state()->pc);
  }

  reg_t custom2(processor_t *p, rocc_insn_t insn, reg_t xs1, reg_t xs2) override {
    return buckyball_exec(insn.funct, xs1, xs2, p->get_state()->pc);
  }

  reg_t custom3(processor_t *p, rocc_insn_t insn, reg_t xs1, reg_t xs2) override {
    return buckyball_exec(insn.funct, xs1, xs2, p->get_state()->pc);
  }
};

// Export the factory function so spike.cc can call it directly
std::function<extension_t*()> buckyball_extension_factory = []() {
  static buckyball_rocc_t ext;
  return &ext;
};
