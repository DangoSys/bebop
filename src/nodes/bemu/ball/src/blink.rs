//! Everything a Ball model sees of the accelerator that hosts it, the emulator side of BLink:
//! the fields of the Ball instruction, the bank geometry, the Ball's own route and parameters
//! and its traces. A Ball model uses `ExecContext` and this module only.

// The Ball instruction: bank operands, iteration count and write mode in rs1.
pub use generic::blink::{pbank, pbank_group, rs1_b0, rs1_b1, rs1_b2, rs1_iter, rs1_read_old, rs1_update};

// The banks: their geometry, the bank storage an instruction reads and writes, and the
// helpers that read and write matrices in them.
pub use generic::bank_matrix::{
    read_i32_nn, read_i32_nn_at, read_i32_nn_groups, read_i32_nn_groups_at, read_i8_k_rows, read_i8_nn, read_i8_nn_at,
    write_i32_nn, write_i32_nn_groups, write_i32_nn_groups_at,
};
pub use generic::banks::{BankConfig, PrivateBank, TrackedBanks, MATRIX_SIZE};
pub use generic::configs::{bank_lines, bank_row_bytes, bank_size, bank_width, is_shared_vbank};

// The Ball's route: its mnemonics and its parameter message.
pub use generic::configs::ball_domain::{mnemonic_for_funct, parameters};

// The Ball's traces.
pub use generic::trace::{ctrace, mtrace, mtrace_enabled, MTraceEvent};

// An expanding Ball has the host execute the instructions it expands into (arch: Expansion into
// the SubROB). No other Ball issues host instructions.
pub use generic::blink::execute_known;
