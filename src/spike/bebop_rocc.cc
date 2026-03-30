#include <atomic>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <stdexcept>

#include <fcntl.h>
#include <sched.h>
#include <sys/mman.h>
#include <unistd.h>

#include "bebop_shm.h"
#include "riscv/mmu.h"

static_assert(sizeof(bebop_shm_t) <= BEBOP_SHM_SIZE);
#include "riscv/rocc.h"

namespace {
constexpr uint64_t kBlockSz = 16;

static void rpc_wait_idle(bebop_lane_t *s) {
  for (;;) {
    uint64_t r = std::atomic_ref(s->req).load(std::memory_order_acquire);
    uint64_t a = std::atomic_ref(s->ack).load(std::memory_order_acquire);
    if (r == a) {
      return;
    }
    sched_yield();
  }
}

static bool env_is_one(const char *name) {
  const char *v = std::getenv(name);
  return v && v[0] == '1';
}

static void service_one_mem(bebop_lane_t *mem, processor_t *proc, uint32_t self_id) {
  uint64_t r = std::atomic_ref(mem->req).load(std::memory_order_acquire);
  uint64_t a = std::atomic_ref(mem->ack).load(std::memory_order_acquire);
  if (r == a) {
    return;
  }
  if (r != a + 1) {
    throw std::runtime_error("bebop_shm invalid mem req/ack");
  }
  auto *mmu = proc->get_mmu();
  if (!mmu) {
    throw std::runtime_error("Spike MMU is null");
  }
  if (mem->msg.op != BEBOP_OP_MEM_REQ) {
    throw std::runtime_error("bebop_shm invalid mem op");
  }
  if (mem->msg.size != kBlockSz) {
    throw std::runtime_error("bebop_shm invalid mem size");
  }
  try {
    if (mem->msg.mem_rw == BEBOP_MEM_READ) {
      for (uint64_t j = 0; j < kBlockSz; ++j) {
        mem->msg.data[j] = mmu->load<uint8_t>(mem->msg.addr + j);
      }
      mem->msg.err = 0;
    } else if (mem->msg.mem_rw == BEBOP_MEM_WRITE) {
      for (uint64_t j = 0; j < kBlockSz; ++j) {
        mmu->store<uint8_t>(mem->msg.addr + j, mem->msg.data[j]);
      }
      mem->msg.err = 0;
    } else {
      mem->msg.err = -1;
    }
  } catch (...) {
    mem->msg.err = -1;
  }
  mem->msg.op = BEBOP_OP_MEM_RESP;
  mem->msg.size = kBlockSz;
  mem->msg.receiver_id = mem->msg.sender_id;
  mem->msg.sender_id = self_id;
  std::atomic_ref(mem->ack).store(r, std::memory_order_release);
}

} // namespace

class bebop_rocc_t final : public rocc_t {
public:
  bebop_rocc_t() = default;

  ~bebop_rocc_t() override {
    if (shm_ && shm_ != MAP_FAILED) {
      munmap(shm_, BEBOP_SHM_SIZE);
      shm_ = nullptr;
    }
  }

  const char *name() const override { return "bebop_rocc"; }

  reg_t custom0(processor_t *proc, rocc_insn_t insn, reg_t xs1, reg_t xs2) override {
    init();
    if (!dual_cmd_) {
      if (rtl_only_) {
        return issue_one_lane(proc, insn, xs1, xs2, &shm_->cmd_rtl, &shm_->mem_rtl);
      }
      return issue_one_lane(proc, insn, xs1, xs2, &shm_->cmd_bemu, &shm_->mem_bemu);
    }
    auto *cb = &shm_->cmd_bemu;
    auto *cr = &shm_->cmd_rtl;
    rpc_wait_idle(cb);
    rpc_wait_idle(cr);
    cb->msg.op = BEBOP_OP_CMD_REQ;
    cb->msg.sender_id = self_id_;
    cb->msg.receiver_id = 0;
    cb->msg.cmd_code = BEBOP_CMD_HANDLE;
    cb->msg.msg_id = ++msg_seq_;
    cb->msg.funct = insn.funct;
    cb->msg.xs1 = xs1;
    cb->msg.xs2 = xs2;
    cb->msg.err = 0;
    cb->msg.bank_digest = 0;
    std::memcpy(&cr->msg, &cb->msg, sizeof(bebop_msg_t));
    std::atomic_ref(cb->req).fetch_add(1, std::memory_order_acq_rel);
    std::atomic_ref(cr->req).fetch_add(1, std::memory_order_acq_rel);
    uint64_t tb = std::atomic_ref(cb->req).load(std::memory_order_acquire);
    uint64_t tr = std::atomic_ref(cr->req).load(std::memory_order_acquire);
    while (std::atomic_ref(cb->ack).load(std::memory_order_acquire) != tb ||
           std::atomic_ref(cr->ack).load(std::memory_order_acquire) != tr) {
      service_one_mem(&shm_->mem_bemu, proc, self_id_);
      service_one_mem(&shm_->mem_rtl, proc, self_id_);
      sched_yield();
    }
    if (cb->msg.op != BEBOP_OP_CMD_RESP || cb->msg.err != 0) {
      throw std::runtime_error("bebop_shm CMD_HANDLE failed (bemu lane)");
    }
    if (cr->msg.op != BEBOP_OP_CMD_RESP || cr->msg.err != 0) {
      throw std::runtime_error("bebop_shm CMD_HANDLE failed (rtl lane)");
    }
    if (cb->msg.result != cr->msg.result) {
      throw std::runtime_error("bebop_shm BEMU vs RTL result mismatch");
    }
    if (difftest_ && cb->msg.bank_digest != cr->msg.bank_digest) {
      throw std::runtime_error("bebop_shm BEMU vs RTL bank_digest mismatch");
    }
    return cb->msg.result;
  }

  reg_t custom3(processor_t *proc, rocc_insn_t insn, reg_t xs1, reg_t xs2) override {
    return custom0(proc, insn, xs1, xs2);
  }

private:
  reg_t issue_one_lane(processor_t *proc, rocc_insn_t insn, reg_t xs1, reg_t xs2, bebop_lane_t *cmd,
                       bebop_lane_t *mem) {
    rpc_wait_idle(cmd);
    cmd->msg.op = BEBOP_OP_CMD_REQ;
    cmd->msg.sender_id = self_id_;
    cmd->msg.receiver_id = 0;
    cmd->msg.cmd_code = BEBOP_CMD_HANDLE;
    cmd->msg.msg_id = ++msg_seq_;
    cmd->msg.funct = insn.funct;
    cmd->msg.xs1 = xs1;
    cmd->msg.xs2 = xs2;
    cmd->msg.err = 0;
    cmd->msg.bank_digest = 0;
    std::atomic_ref(cmd->req).fetch_add(1, std::memory_order_acq_rel);
    uint64_t target = std::atomic_ref(cmd->req).load(std::memory_order_acquire);
    while (std::atomic_ref(cmd->ack).load(std::memory_order_acquire) != target) {
      service_one_mem(mem, proc, self_id_);
      sched_yield();
    }
    if (cmd->msg.op != BEBOP_OP_CMD_RESP || cmd->msg.err != 0) {
      throw std::runtime_error("bebop_shm CMD_HANDLE failed");
    }
    return cmd->msg.result;
  }

  void init() {
    if (shm_) {
      return;
    }
    const char *nm = std::getenv("BEBOP_SHM_NAME");
    if (!nm || !*nm) {
      throw std::runtime_error("BEBOP_SHM_NAME is not set");
    }
    int fd = shm_open(nm, O_RDWR, 0);
    if (fd < 0) {
      throw std::runtime_error("shm_open(BEBOP_SHM_NAME) failed");
    }
    void *p = mmap(nullptr, BEBOP_SHM_SIZE, PROT_READ | PROT_WRITE, MAP_SHARED, fd, 0);
    close(fd);
    if (p == MAP_FAILED) {
      throw std::runtime_error("mmap bebop shm failed");
    }
    shm_ = static_cast<bebop_shm_t *>(p);
    const char *self = std::getenv("BEBOP_NODE_ID");
    if (!self || !*self) {
      throw std::runtime_error("BEBOP_NODE_ID is not set");
    }
    self_id_ = static_cast<uint32_t>(std::strtoul(self, nullptr, 10));
    if (self_id_ == 0) {
      throw std::runtime_error("invalid BEBOP_NODE_ID");
    }
    dual_cmd_ = env_is_one("BEBOP_DUAL_CMD");
    rtl_only_ = env_is_one("BEBOP_RTL_ONLY");
    difftest_ = env_is_one("BEBOP_DIFFTEST");
  }

  bebop_shm_t *shm_ = nullptr;
  uint64_t msg_seq_ = 0;
  uint32_t self_id_ = 0;
  bool dual_cmd_ = false;
  bool rtl_only_ = false;
  bool difftest_ = false;
};

REGISTER_EXTENSION(bebop_rocc, []() { return new bebop_rocc_t(); })
