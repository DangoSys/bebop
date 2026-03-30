// Top for Verilator: wraps Chisel-generated `BebopSpikeCosimTop` from arch (`src/verilator/gen/*.sv`).
// Regenerate: `scripts/emit-arch-cosim-verilog.sh` (from arch via mill).
module bebop_accel (
  input wire clk,
  input wire digest_all_banks,
  input wire [6:0] funct,
  input wire [63:0] xs1,
  input wire [63:0] xs2,
  output wire [63:0] result,
  output wire [63:0] bank_digest_peek
);

  wire [63:0] chisel_bank_digest_unused;

  BebopSpikeCosimTop u_arch (
    .funct           (funct),
    .xs1             (xs1),
    .xs2             (xs2),
    .result          (result),
    .bankDigestPeek  (chisel_bank_digest_unused)
  );

  bebop_cosim_banks u_banks (
    .clk               (clk),
    .digest_all_banks  (digest_all_banks),
    .funct             (funct),
    .xs1               (xs1),
    .xs2               (xs2),
    .bank_digest_peek  (bank_digest_peek)
  );

endmodule
