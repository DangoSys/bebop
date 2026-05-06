// Copyright 1986-2022 Xilinx, Inc. All Rights Reserved.
// --------------------------------------------------------------------------------
// Tool Version: Vivado v.2022.1_AR000034905_AR34035 (lin64) Build 3526262 Mon Apr 18 15:47:01 MDT 2022
// Date        : Thu Jun  5 10:08:29 2025
// Host        : hw_server02 running 64-bit CentOS Linux release 7.9.2009 (Core)
// Command     : write_verilog -force -mode synth_stub
//               ../../../netlist_macro_packages/xepic_ddr4_dc1/stub/xepic_ddr4_dc1_stub.sv
// Design      : xepic_ddr4_dc1
// Purpose     : Stub declaration of top-level module interface
// Device      : xcvu19p-fsva3824-1-e
// --------------------------------------------------------------------------------

// This empty module with port declaration file causes synthesis tools to infer a black box for IP.
// The synthesis directives are for Synopsys Synplify support to prevent IO buffer insertion.
// Please paste the declaration into a Verilog source file or add the file as an additional source.
module xepic_ddr4_dc1(sys_rstn, c0_sys_clk_p, c0_sys_clk_n, 
  c0_ddr4_act_n, c0_ddr4_adr, c0_ddr4_ba, c0_ddr4_bg, c0_ddr4_cke, c0_ddr4_odt, c0_ddr4_cs_n, 
  c0_ddr4_ck_t, c0_ddr4_ck_c, c0_ddr4_reset_n, c0_ddr4_dm_dbi_n, c0_ddr4_dq, c0_ddr4_dqs_c, 
  c0_ddr4_dqs_t, gclk_100m, ddr4_en_vtt_bbox, ddr4_en_vddq_bbox, ddr4_en_vcc2v5_bbox, 
  power_good_bbox, init_start, init_cfg, init_busy, init_calib_complete, 
  c0_init_calib_complete, axi_clk, s0_ddr4_s_axi_awid, s0_ddr4_s_axi_awaddr, 
  s0_ddr4_s_axi_awlen, s0_ddr4_s_axi_awsize, s0_ddr4_s_axi_awburst, s0_ddr4_s_axi_awlock, 
  s0_ddr4_s_axi_awcache, s0_ddr4_s_axi_awprot, s0_ddr4_s_axi_awqos, 
  s0_ddr4_s_axi_awvalid, s0_ddr4_s_axi_awready, s0_ddr4_s_axi_wdata, s0_ddr4_s_axi_wstrb, 
  s0_ddr4_s_axi_wlast, s0_ddr4_s_axi_wvalid, s0_ddr4_s_axi_wready, s0_ddr4_s_axi_bready, 
  s0_ddr4_s_axi_bid, s0_ddr4_s_axi_bresp, s0_ddr4_s_axi_bvalid, s0_ddr4_s_axi_arid, 
  s0_ddr4_s_axi_araddr, s0_ddr4_s_axi_arlen, s0_ddr4_s_axi_arsize, s0_ddr4_s_axi_arburst, 
  s0_ddr4_s_axi_arlock, s0_ddr4_s_axi_arcache, s0_ddr4_s_axi_arprot, s0_ddr4_s_axi_arqos, 
  s0_ddr4_s_axi_arvalid, s0_ddr4_s_axi_arready, s0_ddr4_s_axi_rready, s0_ddr4_s_axi_rid, 
  s0_ddr4_s_axi_rdata, s0_ddr4_s_axi_rresp, s0_ddr4_s_axi_rlast, s0_ddr4_s_axi_rvalid, 
  c0_ddr4_ui_clk, s1_ddr4_s_axi_awid, s1_ddr4_s_axi_awaddr, s1_ddr4_s_axi_awlen, 
  s1_ddr4_s_axi_awsize, s1_ddr4_s_axi_awburst, s1_ddr4_s_axi_awlock, 
  s1_ddr4_s_axi_awcache, s1_ddr4_s_axi_awprot, s1_ddr4_s_axi_awqos, 
  s1_ddr4_s_axi_awvalid, s1_ddr4_s_axi_awready, s1_ddr4_s_axi_wdata, s1_ddr4_s_axi_wstrb, 
  s1_ddr4_s_axi_wlast, s1_ddr4_s_axi_wvalid, s1_ddr4_s_axi_wready, s1_ddr4_s_axi_bready, 
  s1_ddr4_s_axi_bid, s1_ddr4_s_axi_bresp, s1_ddr4_s_axi_bvalid, s1_ddr4_s_axi_arid, 
  s1_ddr4_s_axi_araddr, s1_ddr4_s_axi_arlen, s1_ddr4_s_axi_arsize, s1_ddr4_s_axi_arburst, 
  s1_ddr4_s_axi_arlock, s1_ddr4_s_axi_arcache, s1_ddr4_s_axi_arprot, s1_ddr4_s_axi_arqos, 
  s1_ddr4_s_axi_arvalid, s1_ddr4_s_axi_arready, s1_ddr4_s_axi_rready, s1_ddr4_s_axi_rid, 
  s1_ddr4_s_axi_rdata, s1_ddr4_s_axi_rresp, s1_ddr4_s_axi_rlast, s1_ddr4_s_axi_rvalid)
/* synthesis syn_black_box black_box_pad_pin="sys_rstn,c0_sys_clk_p,c0_sys_clk_n,c0_ddr4_act_n,c0_ddr4_adr[16:0],c0_ddr4_ba[1:0],c0_ddr4_bg[1:0],c0_ddr4_cke[1:0],c0_ddr4_odt[1:0],c0_ddr4_cs_n[1:0],c0_ddr4_ck_t[1:0],c0_ddr4_ck_c[1:0],c0_ddr4_reset_n,c0_ddr4_dm_dbi_n[7:0],c0_ddr4_dq[63:0],c0_ddr4_dqs_c[7:0],c0_ddr4_dqs_t[7:0],gclk_100m,ddr4_en_vtt_bbox,ddr4_en_vddq_bbox,ddr4_en_vcc2v5_bbox,power_good_bbox,init_start,init_cfg,init_busy,init_calib_complete,c0_init_calib_complete,axi_clk,s0_ddr4_s_axi_awid[10:0],s0_ddr4_s_axi_awaddr[63:0],s0_ddr4_s_axi_awlen[7:0],s0_ddr4_s_axi_awsize[2:0],s0_ddr4_s_axi_awburst[1:0],s0_ddr4_s_axi_awlock[0:0],s0_ddr4_s_axi_awcache[3:0],s0_ddr4_s_axi_awprot[2:0],s0_ddr4_s_axi_awqos[3:0],s0_ddr4_s_axi_awvalid,s0_ddr4_s_axi_awready,s0_ddr4_s_axi_wdata[255:0],s0_ddr4_s_axi_wstrb[31:0],s0_ddr4_s_axi_wlast,s0_ddr4_s_axi_wvalid,s0_ddr4_s_axi_wready,s0_ddr4_s_axi_bready,s0_ddr4_s_axi_bid[10:0],s0_ddr4_s_axi_bresp[1:0],s0_ddr4_s_axi_bvalid,s0_ddr4_s_axi_arid[10:0],s0_ddr4_s_axi_araddr[63:0],s0_ddr4_s_axi_arlen[7:0],s0_ddr4_s_axi_arsize[2:0],s0_ddr4_s_axi_arburst[1:0],s0_ddr4_s_axi_arlock[0:0],s0_ddr4_s_axi_arcache[3:0],s0_ddr4_s_axi_arprot[2:0],s0_ddr4_s_axi_arqos[3:0],s0_ddr4_s_axi_arvalid,s0_ddr4_s_axi_arready,s0_ddr4_s_axi_rready,s0_ddr4_s_axi_rid[10:0],s0_ddr4_s_axi_rdata[255:0],s0_ddr4_s_axi_rresp[1:0],s0_ddr4_s_axi_rlast,s0_ddr4_s_axi_rvalid,c0_ddr4_ui_clk,s1_ddr4_s_axi_awid[3:0],s1_ddr4_s_axi_awaddr[63:0],s1_ddr4_s_axi_awlen[7:0],s1_ddr4_s_axi_awsize[2:0],s1_ddr4_s_axi_awburst[1:0],s1_ddr4_s_axi_awlock[0:0],s1_ddr4_s_axi_awcache[3:0],s1_ddr4_s_axi_awprot[2:0],s1_ddr4_s_axi_awqos[3:0],s1_ddr4_s_axi_awvalid,s1_ddr4_s_axi_awready,s1_ddr4_s_axi_wdata[255:0],s1_ddr4_s_axi_wstrb[31:0],s1_ddr4_s_axi_wlast,s1_ddr4_s_axi_wvalid,s1_ddr4_s_axi_wready,s1_ddr4_s_axi_bready,s1_ddr4_s_axi_bid[3:0],s1_ddr4_s_axi_bresp[1:0],s1_ddr4_s_axi_bvalid,s1_ddr4_s_axi_arid[3:0],s1_ddr4_s_axi_araddr[63:0],s1_ddr4_s_axi_arlen[7:0],s1_ddr4_s_axi_arsize[2:0],s1_ddr4_s_axi_arburst[1:0],s1_ddr4_s_axi_arlock[0:0],s1_ddr4_s_axi_arcache[3:0],s1_ddr4_s_axi_arprot[2:0],s1_ddr4_s_axi_arqos[3:0],s1_ddr4_s_axi_arvalid,s1_ddr4_s_axi_arready,s1_ddr4_s_axi_rready,s1_ddr4_s_axi_rid[3:0],s1_ddr4_s_axi_rdata[255:0],s1_ddr4_s_axi_rresp[1:0],s1_ddr4_s_axi_rlast,s1_ddr4_s_axi_rvalid" */;
  input sys_rstn;
  input c0_sys_clk_p;
  input c0_sys_clk_n;
  output c0_ddr4_act_n;
  output [16:0]c0_ddr4_adr;
  output [1:0]c0_ddr4_ba;
  output [1:0]c0_ddr4_bg;
  output [1:0]c0_ddr4_cke;
  output [1:0]c0_ddr4_odt;
  output [1:0]c0_ddr4_cs_n;
  output [1:0]c0_ddr4_ck_t;
  output [1:0]c0_ddr4_ck_c;
  output c0_ddr4_reset_n;
  inout [7:0]c0_ddr4_dm_dbi_n;
  inout [63:0]c0_ddr4_dq;
  inout [7:0]c0_ddr4_dqs_c;
  inout [7:0]c0_ddr4_dqs_t;
  input gclk_100m;
  output ddr4_en_vtt_bbox;
  output ddr4_en_vddq_bbox;
  output ddr4_en_vcc2v5_bbox;
  input power_good_bbox;
  input init_start;
  input init_cfg;
  output init_busy;
  output init_calib_complete;
  output c0_init_calib_complete;
  input axi_clk;
  input [10:0]s0_ddr4_s_axi_awid;
  input [63:0]s0_ddr4_s_axi_awaddr;
  input [7:0]s0_ddr4_s_axi_awlen;
  input [2:0]s0_ddr4_s_axi_awsize;
  input [1:0]s0_ddr4_s_axi_awburst;
  input [0:0]s0_ddr4_s_axi_awlock;
  input [3:0]s0_ddr4_s_axi_awcache;
  input [2:0]s0_ddr4_s_axi_awprot;
  input [3:0]s0_ddr4_s_axi_awqos;
  input s0_ddr4_s_axi_awvalid;
  output s0_ddr4_s_axi_awready;
  input [255:0]s0_ddr4_s_axi_wdata;
  input [31:0]s0_ddr4_s_axi_wstrb;
  input s0_ddr4_s_axi_wlast;
  input s0_ddr4_s_axi_wvalid;
  output s0_ddr4_s_axi_wready;
  input s0_ddr4_s_axi_bready;
  output [10:0]s0_ddr4_s_axi_bid;
  output [1:0]s0_ddr4_s_axi_bresp;
  output s0_ddr4_s_axi_bvalid;
  input [10:0]s0_ddr4_s_axi_arid;
  input [63:0]s0_ddr4_s_axi_araddr;
  input [7:0]s0_ddr4_s_axi_arlen;
  input [2:0]s0_ddr4_s_axi_arsize;
  input [1:0]s0_ddr4_s_axi_arburst;
  input [0:0]s0_ddr4_s_axi_arlock;
  input [3:0]s0_ddr4_s_axi_arcache;
  input [2:0]s0_ddr4_s_axi_arprot;
  input [3:0]s0_ddr4_s_axi_arqos;
  input s0_ddr4_s_axi_arvalid;
  output s0_ddr4_s_axi_arready;
  input s0_ddr4_s_axi_rready;
  output [10:0]s0_ddr4_s_axi_rid;
  output [255:0]s0_ddr4_s_axi_rdata;
  output [1:0]s0_ddr4_s_axi_rresp;
  output s0_ddr4_s_axi_rlast;
  output s0_ddr4_s_axi_rvalid;
  output c0_ddr4_ui_clk;
  input [3:0]s1_ddr4_s_axi_awid;
  input [63:0]s1_ddr4_s_axi_awaddr;
  input [7:0]s1_ddr4_s_axi_awlen;
  input [2:0]s1_ddr4_s_axi_awsize;
  input [1:0]s1_ddr4_s_axi_awburst;
  input [0:0]s1_ddr4_s_axi_awlock;
  input [3:0]s1_ddr4_s_axi_awcache;
  input [2:0]s1_ddr4_s_axi_awprot;
  input [3:0]s1_ddr4_s_axi_awqos;
  input s1_ddr4_s_axi_awvalid;
  output s1_ddr4_s_axi_awready;
  input [255:0]s1_ddr4_s_axi_wdata;
  input [31:0]s1_ddr4_s_axi_wstrb;
  input s1_ddr4_s_axi_wlast;
  input s1_ddr4_s_axi_wvalid;
  output s1_ddr4_s_axi_wready;
  input s1_ddr4_s_axi_bready;
  output [3:0]s1_ddr4_s_axi_bid;
  output [1:0]s1_ddr4_s_axi_bresp;
  output s1_ddr4_s_axi_bvalid;
  input [3:0]s1_ddr4_s_axi_arid;
  input [63:0]s1_ddr4_s_axi_araddr;
  input [7:0]s1_ddr4_s_axi_arlen;
  input [2:0]s1_ddr4_s_axi_arsize;
  input [1:0]s1_ddr4_s_axi_arburst;
  input [0:0]s1_ddr4_s_axi_arlock;
  input [3:0]s1_ddr4_s_axi_arcache;
  input [2:0]s1_ddr4_s_axi_arprot;
  input [3:0]s1_ddr4_s_axi_arqos;
  input s1_ddr4_s_axi_arvalid;
  output s1_ddr4_s_axi_arready;
  input s1_ddr4_s_axi_rready;
  output [3:0]s1_ddr4_s_axi_rid;
  output [255:0]s1_ddr4_s_axi_rdata;
  output [1:0]s1_ddr4_s_axi_rresp;
  output s1_ddr4_s_axi_rlast;
  output s1_ddr4_s_axi_rvalid;
endmodule
