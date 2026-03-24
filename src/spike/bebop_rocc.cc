#include <atomic>
#include <cstdint>
#include <cstdlib>
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
    auto *cmd = &shm_->cmd;
    // step 1. submit command to NPU cmd lane
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
    std::atomic_ref(cmd->req).fetch_add(1, std::memory_order_acq_rel);
    uint64_t target = std::atomic_ref(cmd->req).load(std::memory_order_acquire);
    // step 2. block on cmd response, but keep serving mem lane requests
    while (std::atomic_ref(cmd->ack).load(std::memory_order_acquire) != target) {
      service_mem_req(proc);
      sched_yield();
    }
    if (cmd->msg.op != BEBOP_OP_CMD_RESP || cmd->msg.err != 0) {
      throw std::runtime_error("bebop_shm CMD_HANDLE failed");
    }
    return cmd->msg.result;
  }

  reg_t custom3(processor_t *proc, rocc_insn_t insn, reg_t xs1, reg_t xs2) override {
    return custom0(proc, insn, xs1, xs2);
  }

private:
  void service_mem_req(processor_t *proc) {
    auto *mem = &shm_->mem;
    // NPU can issue mem requests while Spike is waiting for cmd unlock.
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
    mem->msg.sender_id = self_id_;
    std::atomic_ref(mem->ack).store(r, std::memory_order_release);
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
  }

  bebop_shm_t *shm_ = nullptr;
  uint64_t msg_seq_ = 0;
  uint32_t self_id_ = 0;
};

REGISTER_EXTENSION(bebop_rocc, []() { return new bebop_rocc_t(); })
