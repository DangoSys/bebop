// Top for Verilator: wraps Chisel-generated `BebopSpikeCosimTop` from arch (`src/verilator/gen/*.sv`).
// Regenerate: `scripts/emit-arch-cosim-verilog.sh` (from arch via mill).
module bebop_accel (
  input wire clk,
  input wire digest_all_banks,
  input wire [6:0] funct,
  input wire [63:0] xs1,
  input wire [63:0] xs2,
  output logic [63:0] result,
  output wire [63:0] bank_digest_peek
);

  function automatic bit is_known_funct(input logic [6:0] f);
    begin
      unique case (f)
        7'd0,   // fence
        7'd1,   // barrier
        7'd2,   // gemmini_config
        7'd3,   // gemmini_flush
        7'd4,   // bdb_counter
        7'd16,  // mvout
        7'd32,  // mset
        7'd33,  // mvin
        7'd48,  // im2col
        7'd49,  // transpose
        7'd50,  // relu
        7'd51,  // quant
        7'd52,  // dequant
        7'd53,  // gemmini_preload
        7'd54,  // bdb_backdoor
        7'd64,  // mul_warp16
        7'd65,  // bfp
        7'd66,  // gemmini_compute_preloaded
        7'd67,  // gemmini_compute_accumulated
        7'd80, 7'd81, 7'd82, 7'd83, 7'd84, 7'd85, 7'd86, 7'd87,        // loop ws
        7'd96, 7'd97, 7'd98, 7'd99, 7'd100, 7'd101, 7'd102, 7'd103, 7'd104, 7'd105: begin
          is_known_funct = 1'b1;
        end
        default: begin
          is_known_funct = 1'b0;
        end
      endcase
    end
  endfunction

  always_comb begin
    if (is_known_funct(funct)) begin
      result = {57'd0, funct};
    end else begin
      result = 64'd0;
    end
  end

  always_ff @(posedge clk) begin
    if (!is_known_funct(funct))
      $fatal(1, "bebop_accel: unknown funct=%0d", funct);
  end

  bebop_cosim_banks u_banks (
    .clk               (clk),
    .digest_all_banks  (digest_all_banks),
    .funct             (funct),
    .xs1               (xs1),
    .xs2               (xs2),
    .bank_digest_peek  (bank_digest_peek)
  );

endmodule
