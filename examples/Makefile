# Build RISC-V test programs and run with Spike + BEMU.
# Prereqs: RISC-V gcc with newlib (or use repo prebuilt via build-and-install-pk.sh), spike (with bebop_rocc), pk, libbemu.so.

BEMU_ROOT = $(dir $(abspath $(lastword $(MAKEFILE_LIST))))..

# Prefer repo prebuilt toolchain (has newlib); else riscv64-unknown-elf-; else riscv64-linux-gnu-
PREBUILT_GCC := $(shell for d in $(BEMU_ROOT)/.cache/riscv-toolchain/xpack-*/bin; do [ -x "$$d/riscv-none-elf-gcc" ] && echo "$$d/riscv-none-elf-gcc" && break; done)
ifneq ($(PREBUILT_GCC),)
  CC = $(PREBUILT_GCC)
  CFLAGS = -Wall -O2 -march=rv64gc_zicsr_zifencei -mabi=lp64
else
  RISCV_PREFIX ?= $(shell which riscv64-unknown-elf-gcc >/dev/null 2>&1 && echo riscv64-unknown-elf- || echo riscv64-linux-gnu-)
  CC = $(RISCV_PREFIX)gcc
  CFLAGS = -Wall -O2 -march=rv64gc
endif

LIBBEMU = $(BEMU_ROOT)/target/release/libbemu.so
SPIKE_EXT = --extension=bebop_rocc

TESTS = test_bemu_custom test_bemu_mvin_mvout test_bemu_matmul test_bemu_transpose test_bemu_integration
.PHONY: all clean run run-all test_bemu_custom libbemu spike-build install-pk $(TESTS)

all: $(TESTS)

libbemu:
	cd "$(BEMU_ROOT)" && cargo build --release
	@test -f "$(LIBBEMU)" || (echo "ERROR: $(LIBBEMU) not found"; exit 1)

test_bemu_custom: test_bemu_custom.c bebop_insn.h
	$(CC) $(CFLAGS) -o $@ test_bemu_custom.c

test_bemu_mvin_mvout: test_bemu_mvin_mvout.c bebop_insn.h
	$(CC) $(CFLAGS) -o $@ test_bemu_mvin_mvout.c

test_bemu_matmul: test_bemu_matmul.c bebop_insn.h
	$(CC) $(CFLAGS) -o $@ test_bemu_matmul.c

test_bemu_transpose: test_bemu_transpose.c bebop_insn.h
	$(CC) $(CFLAGS) -o $@ test_bemu_transpose.c

test_bemu_integration: test_bemu_integration.c bebop_insn.h
	$(CC) $(CFLAGS) -o $@ test_bemu_integration.c

# Use repo-built Spike so libcustomext.so includes bebop_rocc. Override SPIKE/PK if needed.
SPIKE_BUILD = $(BEMU_ROOT)/thirdparty/spike/build
SPIKE_INSTALL = $(SPIKE_BUILD)/install
SPIKE ?= $(SPIKE_BUILD)/spike
# Auto-detect pk: current dir, then spike install bin, then $RISCV/bin (so make run works without setting PK)
PK_AUTO := $(shell for d in . "$(SPIKE_INSTALL)/riscv64-unknown-elf/bin" "$${RISCV:-/none}/bin"; do [ -x "$$d/pk" ] && echo "$$d/pk" && break; done)
PK ?= $(PK_AUTO)
run: test_bemu_custom libbemu
	@test -x "$(SPIKE)" || { echo "ERROR: Build Spike first: cd $(BEMU_ROOT) && ./scripts/build-spike-bemu.sh"; exit 1; }
	@test -n "$(PK)" && test -x "$(PK)" || { \
	  echo "ERROR: pk not found. Either:"; \
	  echo "  1) Copy/link riscv-pk build to: $(SPIKE_INSTALL)/riscv64-unknown-elf/bin/pk"; \
	  echo "  2) Or run: make run PK=/path/to/pk"; \
	  exit 1; }
	LD_LIBRARY_PATH="$(BEMU_ROOT)/target/release:$(SPIKE_BUILD):$$LD_LIBRARY_PATH" $(SPIKE) $(SPIKE_EXT) $(PK) ./test_bemu_custom

run-all: $(TESTS) libbemu
	@test -x "$(SPIKE)" || { echo "ERROR: Build Spike first: cd $(BEMU_ROOT) && ./scripts/build-spike-bemu.sh"; exit 1; }
	@test -n "$(PK)" && test -x "$(PK)" || { echo "ERROR: pk not found. Set PK= and run make run-all"; exit 1; }
	@export LD_LIBRARY_PATH="$(BEMU_ROOT)/target/release:$(SPIKE_BUILD):$$LD_LIBRARY_PATH"; \
	for t in $(TESTS); do \
	  echo "--- Running $$t ---"; \
	  $(SPIKE) $(SPIKE_EXT) $(PK) ./$$t || exit 1; \
	done; \
	echo "--- All tests passed ---"

# One-time: install pk so "make run" finds it automatically. Usage: make install-pk PK=/home/daiyongyuan/riscv-pk/build/pk
install-pk:
	@if [ -z "$(PK)" ]; then echo "ERROR: PK not set. Run: make install-pk PK=/home/daiyongyuan/riscv-pk/build/pk"; exit 1; fi; \
	if [ ! -f "$(PK)" ]; then echo "ERROR: File not found: $(PK). Build riscv-pk first (e.g. cd riscv-pk/build && make)"; exit 1; fi; \
	if [ ! -x "$(PK)" ]; then echo "ERROR: Not executable: $(PK). Run: chmod +x $(PK)"; exit 1; fi; \
	mkdir -p "$(SPIKE_INSTALL)/riscv64-unknown-elf/bin" && \
	cp "$(PK)" "$(SPIKE_INSTALL)/riscv64-unknown-elf/bin/pk" && \
	echo "Installed pk to $(SPIKE_INSTALL)/riscv64-unknown-elf/bin/pk — you can now use 'make run'."

clean:
	rm -f $(TESTS)
