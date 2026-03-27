cmake_minimum_required(VERSION 3.16)
project(bebop_workload NONE)

find_program(SPIKE_EXE spike REQUIRED)
# Resolve symlinks (e.g. /usr/bin/spike -> nix store); else SPIKE_ROOT becomes /usr and mixes wrong headers with the compiler.
get_filename_component(SPIKE_EXE_REAL "${SPIKE_EXE}" REALPATH)
get_filename_component(SPIKE_BIN_DIR "${SPIKE_EXE_REAL}" DIRECTORY)
get_filename_component(SPIKE_ROOT "${SPIKE_BIN_DIR}" DIRECTORY)
set(SPIKE_LIB_DIR "${SPIKE_ROOT}/lib")

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
