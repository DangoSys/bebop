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
    return execute(p, insn, xs1, xs2);
  }

  reg_t custom1(processor_t *p, rocc_insn_t insn, reg_t xs1, reg_t xs2) override {
    return execute(p, insn, xs1, xs2);
  }

  reg_t custom2(processor_t *p, rocc_insn_t insn, reg_t xs1, reg_t xs2) override {
    return execute(p, insn, xs1, xs2);
  }

  reg_t custom3(processor_t *p, rocc_insn_t insn, reg_t xs1, reg_t xs2) override {
    return execute(p, insn, xs1, xs2);
  }

 private:
  reg_t execute(processor_t *p, rocc_insn_t insn, reg_t xs1, reg_t xs2) {
    auto *state = p->get_state();
    // MVOUT, MVIN, MVIN_2D and MVIN_MMIO share one translation context
    // for the whole instruction, including transfers across page boundaries.
    const bool dma = insn.funct == 16 || insn.funct == 33 ||
                     insn.funct == 34 || insn.funct == 35;
    if (!dma)
      return buckyball_exec(current_bemu_state(), insn.funct, xs1, xs2, state->pc);

    const reg_t previous_privilege = state->prv;
    const reg_t previous_mstatus = state->mstatus->read();
    state->mstatus->write(previous_mstatus | MSTATUS_SUM);
    p->set_privilege(PRV_S, false);
    const reg_t result =
        buckyball_exec(current_bemu_state(), insn.funct, xs1, xs2, state->pc);
    p->set_privilege(previous_privilege, false);
    state->mstatus->write(previous_mstatus);
    return result;
  }
};

// Export the factory function so spike.cc can call it directly
std::function<extension_t*()> buckyball_extension_factory = []() {
  return new buckyball_rocc_t();
};
