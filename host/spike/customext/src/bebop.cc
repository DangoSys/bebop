#include "bebop.h"
#include "ipc/socket.h"
#include <cassert>
#include <cstdio>
#include <riscv/mmu.h>
#include <riscv/trap.h>

using namespace std;

REGISTER_EXTENSION(bebop, []() { return new bebop_t; })

bebop_t::bebop_t() : socket_client(new SocketClient()) {}

bebop_t::~bebop_t() {
  // socket_client will be automatically destroyed
}

#define dprintf(...)                                                           \
  {                                                                            \
    if (p->get_log_commits_enabled())                                          \
      printf(__VA_ARGS__);                                                     \
  }

template <class T> T bebop_t::read_from_dram(reg_t addr) {
  T value = 0;
  for (size_t byte_idx = 0; byte_idx < sizeof(T); ++byte_idx) {
    // Cast to unsigned to avoid sign extension
    uint8_t byte_val = (uint8_t)p->get_mmu()->load<uint8_t>(addr + byte_idx);
    value |= ((T)byte_val) << (byte_idx * 8);
  }
  return value;
}

template <class T> void bebop_t::write_to_dram(reg_t addr, T data) {
  for (size_t byte_idx = 0; byte_idx < sizeof(T); ++byte_idx) {
    p->get_mmu()->store<uint8_t>(addr + byte_idx,
                                 (data >> (byte_idx * 8)) & 0xFF);
  }
}

void bebop_state_t::reset() {
  enable = true;
  resetted = true;
}

reg_t bebop_t::CUSTOMFN(XCUSTOM_ACC)(rocc_insn_t insn, reg_t xs1, reg_t xs2) {

  if (!bebop_state.resetted) {
    bebop_state.reset();
  }

  auto read_cb = [this](uint64_t addr, uint32_t size) -> dma_data_128_t {
    dma_data_128_t value{};
    // printf("[BEBOP] DMA read callback: addr=0x%lx, size=%u\n", addr, size);
    switch (size) {
    case 1:
      value.lo = read_from_dram<uint8_t>(addr);
      // printf("[BEBOP] Read 1 byte: value.lo=0x%lx\n", value.lo);
      break;
    case 2:
      value.lo = read_from_dram<uint16_t>(addr);
      // printf("[BEBOP] Read 2 bytes: value.lo=0x%lx\n", value.lo);
      break;
    case 4:
      value.lo = read_from_dram<uint32_t>(addr);
      // printf("[BEBOP] Read 4 bytes: value.lo=0x%lx\n", value.lo);
      break;
    case 8:
      value.lo = read_from_dram<uint64_t>(addr);
      // printf("[BEBOP] Read 8 bytes: value.lo=0x%lx\n", value.lo);
      break;
    case 16:
      value.lo = read_from_dram<uint64_t>(addr);
      value.hi = read_from_dram<uint64_t>(addr + 8);
      // printf("[BEBOP] Read 16 bytes: value.lo=0x%lx, value.hi=0x%lx\n", value.lo, value.hi);
      // Print raw bytes
      // printf("[BEBOP] Raw bytes at addr 0x%lx:\n", addr);
      // for (int i = 0; i < 16; i++) {
      //   uint8_t b = read_from_dram<uint8_t>(addr + i);
      //   printf("%02x ", b);
      //   if ((i + 1) % 8 == 0) printf("\n");
      // }
      break;
    default:
      fprintf(stderr, "bebop: Invalid DMA read size %u\n", size);
      abort();
    }
    return value;
  };

  auto write_cb = [this](uint64_t addr, dma_data_128_t data, uint32_t size) {
    switch (size) {
    case 1:
      write_to_dram<uint8_t>(addr, static_cast<uint8_t>(data.lo));
      break;
    case 2:
      write_to_dram<uint16_t>(addr, static_cast<uint16_t>(data.lo));
      break;
    case 4:
      write_to_dram<uint32_t>(addr, static_cast<uint32_t>(data.lo));
      break;
    case 8:
      write_to_dram<uint64_t>(addr, data.lo);
      break;
    case 16:
      write_to_dram<uint64_t>(addr, data.lo);
      write_to_dram<uint64_t>(addr + 8, data.hi);
      break;
    default:
      fprintf(stderr, "bebop: Invalid DMA write size %u\n", size);
      abort();
    }
  };

  socket_client->set_dma_callbacks(read_cb, write_cb);

  // Send socket request and wait for response
  dprintf("bebop: Processing custom instruction with funct=%d\n", insn.funct);
  reg_t result = socket_client->send_and_wait(insn.funct, xs1, xs2);

  dprintf("bebop: custom instruction funct=%d completed with result=0x%lx\n",
          insn.funct, result);

  return result;
}

static reg_t bebop_custom(processor_t *p, insn_t insn, reg_t pc) {
  bebop_t *bebop = static_cast<bebop_t *>(p->get_extension("bebop"));
  rocc_insn_union_t u;
  state_t *state = p->get_state();
  bebop->set_processor(p);
  u.i = insn;
  reg_t xs1 = u.r.xs1 ? state->XPR[insn.rs1()] : -1;
  reg_t xs2 = u.r.xs2 ? state->XPR[insn.rs2()] : -1;
  reg_t xd = bebop->CUSTOMFN(XCUSTOM_ACC)(u.r, xs1, xs2);
  if (u.r.xd) {
    state->log_reg_write[insn.rd() << 4] = {xd, 0};
    state->XPR.write(insn.rd(), xd);
  }
  return pc + 4;
}

std::vector<insn_desc_t> bebop_t::get_instructions(const processor_t &proc) {
  std::vector<insn_desc_t> insns;
  push_custom_insn(insns, ROCC_OPCODE3, ROCC_OPCODE_MASK, ILLEGAL_INSN_FUNC,
                   bebop_custom);
  return insns;
}

std::vector<disasm_insn_t *> bebop_t::get_disasms(const processor_t *proc) {
  std::vector<disasm_insn_t *> insns;
  return insns;
}
