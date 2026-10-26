#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <vpi_user.h>

extern "C" void dpi_bdb_set_clk(unsigned long long) {}
extern "C" void bbsim_memory_print_stats();

extern "C" void scu_uart_write(std::uint32_t, std::uint32_t ch) {
  std::fputc(static_cast<int>(ch & 0xff), stdout);
  std::fflush(stdout);
}

extern "C" void scu_sim_exit(std::uint32_t hart, std::uint32_t code) {
  std::fprintf(stderr, "[SCU] hart %u: simulation exit code %u\n", hart, code);
  std::fflush(stderr);
  bbsim_memory_print_stats();
  // vpiFinish takes a diagnostic level, not the workload's exit status.
  if (code != 0)
    std::exit(EXIT_FAILURE);
  vpi_control(vpiFinish, 0);
}

extern "C" void scu_uart_rx_sample(std::uint32_t, std::uint32_t,
                                   std::uint32_t *valid, std::uint32_t *data) {
  *valid = 0;
  *data = 0;
}

extern "C" void dpi_ant_trace(std::uint32_t, std::uint32_t, std::uint32_t,
                              std::uint32_t, std::uint32_t, std::uint32_t,
                              std::uint32_t, std::uint32_t) {}
extern "C" void dpi_itrace(std::uint32_t, std::uint32_t, std::uint32_t,
                           std::uint32_t, std::uint32_t, std::uint32_t,
                           std::uint32_t, std::uint32_t, std::uint32_t,
                           std::uint32_t, std::uint32_t, std::uint32_t,
                           std::uint32_t, std::uint32_t, std::uint32_t) {}
extern "C" void dpi_mtrace(std::uint32_t, std::uint32_t, std::uint32_t,
                           std::uint32_t, std::uint32_t, std::uint32_t,
                           std::uint32_t, std::uint32_t, std::uint32_t,
                           std::uint32_t, std::uint32_t, std::uint32_t,
                           std::uint32_t, std::uint32_t, std::uint32_t,
                           std::uint32_t) {}
extern "C" void dpi_pmctrace(std::uint32_t, std::uint32_t, std::uint32_t,
                             std::uint32_t) {}
extern "C" void dpi_mem_pmctrace(std::uint32_t, std::uint32_t, std::uint32_t,
                                 std::uint32_t) {}
extern "C" void dpi_mtrace_issue(std::uint32_t, std::uint32_t, std::uint32_t,
                                 std::uint32_t, std::uint32_t, std::uint32_t) {}
