#include <array>
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
constexpr uint32_t kMvin = 24;
constexpr uint32_t kMvout = 25;
constexpr uint64_t kBlockSz = 16;

struct mvin_args_t {
  uint64_t mem_addr;
  uint32_t depth;
  uint32_t stride;
};

static mvin_args_t parse_mem_args(uint64_t xs1, uint64_t xs2) {
  mvin_args_t args{};
  args.mem_addr = (xs1 >> 27) & 0xffffffffULL;
  args.depth = static_cast<uint32_t>(xs2 & 0x3ffULL);
  args.stride = static_cast<uint32_t>((xs2 >> 10) & 0x7ffffULL);
  return args;
}

static void rpc_wait_idle(bebop_shm_t *s) {
  for (;;) {
    uint64_t r = std::atomic_ref(s->req).load(std::memory_order_acquire);
    uint64_t a = std::atomic_ref(s->ack).load(std::memory_order_acquire);
    if (r == a) {
      return;
    }
    sched_yield();
  }
}

static void rpc_wait_done(bebop_shm_t *s) {
  uint64_t r = std::atomic_ref(s->req).load(std::memory_order_acquire);
  while (std::atomic_ref(s->ack).load(std::memory_order_acquire) != r) {
    sched_yield();
  }
}

static void rpc_submit(bebop_shm_t *s) {
  rpc_wait_idle(s);
  std::atomic_ref(s->req).fetch_add(1, std::memory_order_acq_rel);
  rpc_wait_done(s);
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

    if (insn.funct == kMvin) {
      sync_in(proc, xs1, xs2);
    }

    shm_->op = BEBOP_OP_HANDLE;
    shm_->funct = insn.funct;
    shm_->xs1 = xs1;
    shm_->xs2 = xs2;
    shm_->err = 0;
    rpc_submit(shm_);
    if (shm_->err != 0) {
      throw std::runtime_error("bebop_shm OP_HANDLE failed");
    }
    uint64_t out = shm_->result;

    if (insn.funct == kMvout) {
      sync_out(proc, xs1, xs2);
    }
    return out;
  }

private:
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
  }

  void sync_in(processor_t *proc, uint64_t xs1, uint64_t xs2) {
    auto args = parse_mem_args(xs1, xs2);
    auto *mmu = proc->get_mmu();
    if (!mmu) {
      throw std::runtime_error("Spike MMU is null");
    }

    std::array<uint8_t, kBlockSz> buf{};
    for (uint32_t i = 0; i < args.depth; ++i) {
      uint64_t addr = args.mem_addr + static_cast<uint64_t>(i) * args.stride * kBlockSz;
      for (uint64_t j = 0; j < kBlockSz; ++j) {
        buf[j] = mmu->load<uint8_t>(addr + j);
      }
      std::memcpy(shm_->data, buf.data(), buf.size());
      shm_->op = BEBOP_OP_SYNC;
      shm_->sync_addr = addr;
      shm_->err = 0;
      rpc_submit(shm_);
      if (shm_->err != 0) {
        throw std::runtime_error("bebop_shm OP_SYNC failed");
      }
    }
  }

  void sync_out(processor_t *proc, uint64_t xs1, uint64_t xs2) {
    auto args = parse_mem_args(xs1, xs2);
    auto *mmu = proc->get_mmu();
    if (!mmu) {
      throw std::runtime_error("Spike MMU is null");
    }

    std::array<uint8_t, kBlockSz> buf{};
    for (uint32_t i = 0; i < args.depth; ++i) {
      uint64_t addr = args.mem_addr + static_cast<uint64_t>(i) * args.stride * kBlockSz;
      shm_->op = BEBOP_OP_READ;
      shm_->sync_addr = addr;
      shm_->err = 0;
      rpc_submit(shm_);
      if (shm_->err != 0) {
        throw std::runtime_error("bebop_shm OP_READ failed");
      }
      std::memcpy(buf.data(), shm_->data, buf.size());
      for (uint64_t j = 0; j < kBlockSz; ++j) {
        mmu->store<uint8_t>(addr + j, buf[j]);
      }
    }
  }

  bebop_shm_t *shm_ = nullptr;
};

REGISTER_EXTENSION(bebop_rocc, []() { return new bebop_rocc_t(); })
