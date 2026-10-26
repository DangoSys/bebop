`timescale 1ns/1ps
module BBSimVcsHarness;
  reg clock = 1'b0;
  reg reset = 1'b1;

  BBSimHarness dut (.clock(clock), .reset(reset));

  always #0.5 clock = ~clock;

  initial begin
    repeat (10) @(posedge clock);
    @(negedge clock);
    reset = 1'b0;
  end

  initial begin
    if ($test$plusargs("vpd")) begin : enable_vpd
      string vpd_file;
      if (!$value$plusargs("vpd=%s", vpd_file)) vpd_file = "activity.vpd";
      $vcdplusfile(vpd_file);
      $vcdpluson(0, dut);
    end
    if ($test$plusargs("vcd")) begin : enable_vcd
      string vcd_file;
      if (!$value$plusargs("vcd=%s", vcd_file)) vcd_file = "activity.vcd";
      $dumpfile(vcd_file);
      $dumpvars(0, dut);
    end
  end
endmodule
