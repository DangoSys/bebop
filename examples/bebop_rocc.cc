#include <dlfcn.h>
#include <array>
#include <cstdint>
#include <stdexcept>

#include "riscv/mmu.h"
#include "riscv/rocc.h"

namespace {
constexpr uint32_t kMset = 23;
constexpr uint32_t kMvin = 24;
constexpr uint32_t kMvout = 25;
constexpr uint64_t kBlockSz = 16;

using bemu_create_interface_t = void* (*)(bool);
using bemu_free_interface_t = void (*)(void*);
using bemu_handle_custom_t = int (*)(void*, uint32_t, uint64_t, uint64_t, uint64_t*);
using bemu_sync_memory_t = int (*)(void*, uint64_t, const uint8_t*, size_t);
using bemu_read_memory_t = int (*)(void*, uint64_t, uint8_t*, size_t);

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
}  // namespace

class bebop_rocc_t final : public rocc_t {
 public:
  bebop_rocc_t() = default;

  ~bebop_rocc_t() override {
    if (bemu_free_interface_ && iface_) {
      bemu_free_interface_(iface_);
      iface_ = nullptr;
    }
    if (lib_handle_) {
      dlclose(lib_handle_);
      lib_handle_ = nullptr;
    }
  }

  const char* name() const override { return "bebop_rocc"; }

  reg_t custom0(processor_t* proc, rocc_insn_t insn, reg_t xs1, reg_t xs2) override {
    init();

    if (insn.funct == kMvin) {
      sync_in(proc, xs1, xs2);
    }

    uint64_t out = 0;
    int rc = bemu_handle_custom_(iface_, insn.funct, xs1, xs2, &out);
    if (rc != 0) {
      throw std::runtime_error("bemu_handle_custom failed");
    }

    if (insn.funct == kMvout) {
      sync_out(proc, xs1, xs2);
    }
    return out;
  }

 private:
  void init() {
    if (iface_) {
      return;
    }

    lib_handle_ = dlopen("libbemu.so", RTLD_NOW | RTLD_GLOBAL);
    if (!lib_handle_) {
      throw std::runtime_error("dlopen(libbemu.so) failed");
    }

    bemu_create_interface_ = load_sym<bemu_create_interface_t>("bemu_create_interface");
    bemu_free_interface_ = load_sym<bemu_free_interface_t>("bemu_free_interface");
    bemu_handle_custom_ = load_sym<bemu_handle_custom_t>("bemu_handle_custom");
    bemu_sync_memory_ = load_sym<bemu_sync_memory_t>("bemu_sync_memory");
    bemu_read_memory_ = load_sym<bemu_read_memory_t>("bemu_read_memory");

    iface_ = bemu_create_interface_(false);
    if (!iface_) {
      throw std::runtime_error("bemu_create_interface returned null");
    }
  }

  template <typename T>
  T load_sym(const char* name) {
    void* sym = dlsym(lib_handle_, name);
    if (!sym) {
      throw std::runtime_error("dlsym failed");
    }
    return reinterpret_cast<T>(sym);
  }

  void sync_in(processor_t* proc, uint64_t xs1, uint64_t xs2) {
    auto args = parse_mem_args(xs1, xs2);
    auto* mmu = proc->get_mmu();
    if (!mmu) {
      throw std::runtime_error("Spike MMU is null");
    }

    std::array<uint8_t, kBlockSz> buf{};
    for (uint32_t i = 0; i < args.depth; ++i) {
      uint64_t addr = args.mem_addr + static_cast<uint64_t>(i) * args.stride * kBlockSz;
      for (uint64_t j = 0; j < kBlockSz; ++j) {
        buf[j] = mmu->load<uint8_t>(addr + j);
      }
      int rc = bemu_sync_memory_(iface_, addr, buf.data(), buf.size());
      if (rc != 0) {
        throw std::runtime_error("bemu_sync_memory failed");
      }
    }
  }

  void sync_out(processor_t* proc, uint64_t xs1, uint64_t xs2) {
    auto args = parse_mem_args(xs1, xs2);
    auto* mmu = proc->get_mmu();
    if (!mmu) {
      throw std::runtime_error("Spike MMU is null");
    }

    std::array<uint8_t, kBlockSz> buf{};
    for (uint32_t i = 0; i < args.depth; ++i) {
      uint64_t addr = args.mem_addr + static_cast<uint64_t>(i) * args.stride * kBlockSz;
      int rc = bemu_read_memory_(iface_, addr, buf.data(), buf.size());
      if (rc != 0) {
        throw std::runtime_error("bemu_read_memory failed");
      }
      for (uint64_t j = 0; j < kBlockSz; ++j) {
        mmu->store<uint8_t>(addr + j, buf[j]);
      }
    }
  }

  void* lib_handle_ = nullptr;
  void* iface_ = nullptr;

  bemu_create_interface_t bemu_create_interface_ = nullptr;
  bemu_free_interface_t bemu_free_interface_ = nullptr;
  bemu_handle_custom_t bemu_handle_custom_ = nullptr;
  bemu_sync_memory_t bemu_sync_memory_ = nullptr;
  bemu_read_memory_t bemu_read_memory_ = nullptr;
};

REGISTER_EXTENSION(bebop_rocc, []() { return new bebop_rocc_t(); })
