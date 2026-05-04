#include <cstdint>
#include <cstdio>
#include <vector>
#include <mutex>
#include <string>

// Global state for UART log and exit code
static std::vector<uint8_t> g_uart_log;
static int g_exit_code = 0;
static bool g_has_exit = false;
static std::mutex g_state_mutex;

// DPI-C functions called from RTL
extern "C" {

// Called when RTL writes to UART (0x60020000)
void p2e_uart_write(uint32_t hart_id, uint8_t ch) {
    std::lock_guard<std::mutex> lock(g_state_mutex);
    g_uart_log.push_back(ch);

    // Print to stdout immediately
    putchar(ch);
    fflush(stdout);
}

// Called when RTL writes to sim_exit (0x60000000)
void p2e_sim_exit(uint32_t hart_id, int32_t code) {
    std::lock_guard<std::mutex> lock(g_state_mutex);
    g_exit_code = code;
    g_has_exit = true;

    printf("\n[P2E] sim_exit called: hart_id=%u, exit_code=%d\n", hart_id, code);
    fflush(stdout);
}

// Helper functions to query state (can be called from Rust FFI if needed)
bool check_sim_exit() {
    std::lock_guard<std::mutex> lock(g_state_mutex);
    return g_has_exit;
}

int get_exit_code() {
    std::lock_guard<std::mutex> lock(g_state_mutex);
    return g_exit_code;
}

const char* get_uart_log() {
    std::lock_guard<std::mutex> lock(g_state_mutex);
    static std::string log_str;
    log_str = std::string(g_uart_log.begin(), g_uart_log.end());
    return log_str.c_str();
}

void reset_state() {
    std::lock_guard<std::mutex> lock(g_state_mutex);
    g_uart_log.clear();
    g_exit_code = 0;
    g_has_exit = false;
}

} // extern "C"
