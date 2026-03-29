// Top for Verilator: wraps Chisel-generated `BebopSpikeCosimTop` from arch (`src/verilator/gen/*.sv`).
// Regenerate: `scripts/emit-arch-cosim-verilog.sh` (from arch via mill).
module bebop_accel (
  input wire [6:0] funct,
  input wire [63:0] xs1,
  input wire [63:0] xs2,
  output wire [63:0] result
);

  BebopSpikeCosimTop u_arch (
    .funct (funct),
    .xs1   (xs1),
    .xs2   (xs2),
    .result(result)
  );

endmodule
