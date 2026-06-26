#include "rocc.h"
#include "processor.h"

extern "C" {
  void* current_bemu_state();
  void buckyball_init(void* state);
  void buckyball_reset(void* state);
  uint64_t buckyball_exec(void* state, uint8_t funct7, uint64_t xs1, uint64_t xs2, uint64_t pc);
}

class buckyball_rocc_t : public rocc_t {
 public:
  const char* name() const override { return "buckyball"; }

  buckyball_rocc_t() {
    buckyball_init(current_bemu_state());
  }

  void reset(processor_t &) override {
    buckyball_reset(current_bemu_state());
  }

  reg_t custom0(processor_t *p, rocc_insn_t insn, reg_t xs1, reg_t xs2) override {
    return buckyball_exec(current_bemu_state(), insn.funct, xs1, xs2, p->get_state()->pc);
  }

  reg_t custom1(processor_t *p, rocc_insn_t insn, reg_t xs1, reg_t xs2) override {
    return buckyball_exec(current_bemu_state(), insn.funct, xs1, xs2, p->get_state()->pc);
  }

  reg_t custom2(processor_t *p, rocc_insn_t insn, reg_t xs1, reg_t xs2) override {
    return buckyball_exec(current_bemu_state(), insn.funct, xs1, xs2, p->get_state()->pc);
  }

  reg_t custom3(processor_t *p, rocc_insn_t insn, reg_t xs1, reg_t xs2) override {
    return buckyball_exec(current_bemu_state(), insn.funct, xs1, xs2, p->get_state()->pc);
  }
};

// Export the factory function so spike.cc can call it directly
std::function<extension_t*()> buckyball_extension_factory = []() {
  return new buckyball_rocc_t();
};
