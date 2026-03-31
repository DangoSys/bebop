// Cosim top: Chisel BebopBuckyballSubsystemCosim + bebop_cosim_banks digest lane.
// Regenerate RTL: `scripts/emit-arch-cosim-verilog.sh` (mill in arch).
opBuckyballSubsystemCosim ties `result` to 0; RoCC `rd` for cosim matches the
// old BebopSpikeCosimTop encoding (funct in low 7 bits).
module bebop_accel (
  input wire clk,
  input wire digest_all_banks,
  input wire issue_start,
  input wire [6:0] funct,
  input wire [63:0] xs1,
  input wire [63:0] xs2,
  output wire [63:0] result,
  output wire issue_done,
  output wire [63:0] bank_digest_peek,
  output wire rtl_busy
);
  logic [3:0] rst_cnt = 4'hf;
  wire rst = |rst_cnt;
  wire issue_done_raw;
  wire [63:0] rtl_result_unused;

  function automatic bit is_known_funct(input logic [6:0] f);
    begin
      unique case (f)
        7'd0,
        7'd1,
        7'd2,
        7'd3,
        7'd4,
        7'd16,
        7'd32,
        7'd33,
        7'd48,
        7'd49,
        7'd50,
        7'd51,
        7'd52,
        7'd53,
        7'd54,
        7'd64,
        7'd65,
        7'd66,
        7'd67,
        7'd80, 7'd81, 7'd82, 7'd83, 7'd84, 7'd85, 7'd86, 7'd87,
        7'd96, 7'd97, 7'd98, 7'd99, 7'd100, 7'd101, 7'd102, 7'd103, 7'd104, 7'd105: begin
          is_known_funct = 1'b1;
        end
        default: begin
          is_known_funct = 1'b0;
        end
      endcase
    end
  endfunction

  always_ff @(posedge clk) begin
    if (rst_cnt != 4'h0)
      rst_cnt <= rst_cnt - 4'h1;
    if (issue_start && !is_known_funct(funct))
      $fatal(1, "bebop_accel: unknown funct=%0d", funct);
  end

  BebopBuckyballSubsystemCosim u_bb (
    .clock (clk),
    .reset (rst),
    .start (issue_start),
    .funct (funct),
    .xs1   (xs1),
    .xs2   (xs2),
    .done  (issue_done_raw),
    .result(rtl_result_unused)
  );

  assign result = {57'h0, funct};
  assign issue_done = issue_done_raw;

  assign rtl_busy =
    u_bb._acc_io_tl_reader_a_valid
    || u_bb._acc_io_tl_writer_a_valid
    || u_bb._buffer_auto_in_d_valid
    || u_bb._buffer_1_auto_in_d_valid
    || u_bb._ram_auto_in_d_valid
    || u_bb._xbar_auto_anon_in_0_d_valid
    || u_bb._xbar_auto_anon_in_1_d_valid
    || u_bb._acc_io_shared_mem_req_0_write_req_valid
    || u_bb._acc_io_shared_mem_req_0_read_req_valid
    || u_bb._acc_io_shared_mem_req_1_write_req_valid
    || u_bb._acc_io_shared_mem_req_1_read_req_valid
    || u_bb._acc_io_shared_mem_req_2_write_req_valid
    || u_bb._acc_io_shared_mem_req_2_read_req_valid
    || u_bb._acc_io_shared_mem_req_3_write_req_valid
    || u_bb._acc_io_shared_mem_req_3_read_req_valid
    || u_bb._acc_io_shared_mem_req_4_write_req_valid
    || u_bb._acc_io_shared_mem_req_4_read_req_valid
    || u_bb._acc_io_shared_mem_req_5_write_req_valid
    || u_bb._acc_io_shared_mem_req_5_read_req_valid
    || u_bb._acc_io_shared_mem_req_6_write_req_valid
    || u_bb._acc_io_shared_mem_req_6_read_req_valid
    || u_bb._shared_io_mem_req_0_write_resp_valid
    || u_bb._shared_io_mem_req_0_read_resp_valid
    || u_bb._shared_io_mem_req_1_write_resp_valid
    || u_bb._shared_io_mem_req_1_read_resp_valid
    || u_bb._shared_io_mem_req_2_write_resp_valid
    || u_bb._shared_io_mem_req_2_read_resp_valid
    || u_bb._shared_io_mem_req_3_write_resp_valid
    || u_bb._shared_io_mem_req_3_read_resp_valid
    || u_bb._shared_io_mem_req_4_write_resp_valid
    || u_bb._shared_io_mem_req_4_read_resp_valid
    || u_bb._shared_io_mem_req_5_write_resp_valid
    || u_bb._shared_io_mem_req_5_read_resp_valid
    || u_bb._shared_io_mem_req_6_write_resp_valid
    || u_bb._shared_io_mem_req_6_read_resp_valid;

  bebop_cosim_banks u_banks (
    .clk              (clk),
    .digest_all_banks (digest_all_banks),
    .funct            (funct),
    .xs1              (xs1),
    .xs2              (xs2),
    .bank_digest_peek (bank_digest_peek)
  );

endmodule
