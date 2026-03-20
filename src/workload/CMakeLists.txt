cmake_minimum_required(VERSION 3.16)

# Auto-pick RISC-V cross compiler when user did not pass -DCMAKE_C_COMPILER.
if(NOT DEFINED CMAKE_C_COMPILER)
  find_program(RV_GCC riscv64-none-elf-gcc)
  if(NOT RV_GCC)
    find_program(RV_GCC riscv64-unknown-elf-gcc)
  endif()
  if(NOT RV_GCC)
    message(FATAL_ERROR "RISC-V GCC not found. Run `nix develop` first, or set -DCMAKE_C_COMPILER.")
  endif()
  set(CMAKE_C_COMPILER "${RV_GCC}" CACHE FILEPATH "RISC-V C compiler" FORCE)
endif()

# Bare-metal cross compile
set(CMAKE_SYSTEM_NAME Generic)
set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)

project(bebop_workload C)

set(CMAKE_C_STANDARD 11)
set(CMAKE_C_STANDARD_REQUIRED ON)

# Repo root: src/workload -> ../..
set(BEMU_ROOT "${CMAKE_CURRENT_LIST_DIR}/../..")

# Rust crate (bebop CLI, libbemu.so): build with Cargo at BEMU_ROOT only.
find_program(SPIKE_EXE spike)
if(NOT SPIKE_EXE)
  message(FATAL_ERROR "spike not found (needed to link bebop_rocc). Run `nix develop` first.")
endif()

get_filename_component(SPIKE_BIN_DIR "${SPIKE_EXE}" DIRECTORY)
get_filename_component(SPIKE_ROOT "${SPIKE_BIN_DIR}" DIRECTORY)
set(SPIKE_LIB_DIR "${SPIKE_ROOT}/lib")

add_compile_options(-Wall -O2 -march=rv64gc_zicsr_zifencei -mabi=lp64d)
add_link_options(-march=rv64gc_zicsr_zifencei -mabi=lp64d)

set(TESTS
  test_bemu_custom
  test_bemu_mvin_mvout
  test_bemu_matmul
  test_bemu_transpose
  test_bemu_integration
  test_vecunit_tiled_matmul
)

foreach(t IN LISTS TESTS)
  add_executable(${t} "${t}.c")
endforeach()

set(BEBOP_ROCC_SO "${CMAKE_CURRENT_BINARY_DIR}/libbebop_rocc.so")
add_custom_target(bebop_rocc
  COMMAND c++ -std=c++20 -fPIC -shared
    "${CMAKE_CURRENT_LIST_DIR}/bebop_rocc.cc"
    -I"${CMAKE_CURRENT_LIST_DIR}"
    -I"${SPIKE_ROOT}/include"
    -L"${SPIKE_LIB_DIR}"
    -lriscv
    -o "${BEBOP_ROCC_SO}"
  BYPRODUCTS "${BEBOP_ROCC_SO}"
  COMMENT "Building bebop_rocc extension"
)
