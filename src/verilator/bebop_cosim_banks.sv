// Bank RAM + bank-touching RoCC ops for BEMU vs Verilator digest (`emu/inst/decode::execute_known`).
`timescale 1ns/1ns

localparam int BANK_NUM = 32;
localparam int BANK_SZ = 16384;
localparam int I8_STR = 16;
localparam int ACC_STR = 64;

import "DPI-C" function void dpi_mem_read16(
  input longint unsigned addr,
  output longint unsigned lo,
  output longint unsigned hi
);
import "DPI-C" function void dpi_mem_write16(
  input longint unsigned addr,
  input longint unsigned lo,
  input longint unsigned hi
);
import "DPI-C" function byte unsigned bebop_dpi_quant_u8(
  input byte unsigned b0, b1, b2, b3,
  input int unsigned scale_bits
);
import "DPI-C" function void bebop_dpi_dequant_i32_le(
  input byte unsigned v_i8,
  input int unsigned scale_bits,
  output byte unsigned o0, o1, o2, o3
);

module bebop_cosim_banks (
  input wire clk,
  input wire issue_start,
  input wire digest_all_banks,
  input wire [6:0] funct,
  input wire [63:0] xs1,
  input wire [63:0] xs2,
  output logic [63:0] bank_digest_peek,
  output logic banks_busy
);

  (* verilator public_flat_rw *) logic [7:0] bram [0:BANK_NUM*BANK_SZ-1];
  logic slot_valid [0:BANK_NUM-1];
  logic [31:0] slot_vbank [0:BANK_NUM-1];
  logic cfg_alloc [0:BANK_NUM-1];
  logic [63:0] cfg_cols [0:BANK_NUM-1];

  logic g_dataflow;
  logic g_ws_b_valid;
  logic [6:0] g_ws_n;
  logic [7:0] ws_b_store [0:4095];
  logic vec_start;
  logic [15:0] vec_iter;
  logic [7:0] vec_op1 [0:15];
  logic [7:0] vec_op2 [0:15];
  wire [31:0] vec_res [0:15];
  wire vec_valid;
  wire vec_done;

  int mul64_op1_p;
  int mul64_op2_p;
  int mul64_wr_p;
  int mul64_kin;
  int mul64_kk;
  int mul64_row;
  bit mul64_busy;

  VecComputeTop u_vec_compute (
    .clock(clk),
    .reset(1'b0),
    .io_start(vec_start),
    .io_iter(vec_iter),
    .io_op1_0(vec_op1[0]),
    .io_op1_1(vec_op1[1]),
    .io_op1_2(vec_op1[2]),
    .io_op1_3(vec_op1[3]),
    .io_op1_4(vec_op1[4]),
    .io_op1_5(vec_op1[5]),
    .io_op1_6(vec_op1[6]),
    .io_op1_7(vec_op1[7]),
    .io_op1_8(vec_op1[8]),
    .io_op1_9(vec_op1[9]),
    .io_op1_10(vec_op1[10]),
    .io_op1_11(vec_op1[11]),
    .io_op1_12(vec_op1[12]),
    .io_op1_13(vec_op1[13]),
    .io_op1_14(vec_op1[14]),
    .io_op1_15(vec_op1[15]),
    .io_op2_0(vec_op2[0]),
    .io_op2_1(vec_op2[1]),
    .io_op2_2(vec_op2[2]),
    .io_op2_3(vec_op2[3]),
    .io_op2_4(vec_op2[4]),
    .io_op2_5(vec_op2[5]),
    .io_op2_6(vec_op2[6]),
    .io_op2_7(vec_op2[7]),
    .io_op2_8(vec_op2[8]),
    .io_op2_9(vec_op2[9]),
    .io_op2_10(vec_op2[10]),
    .io_op2_11(vec_op2[11]),
    .io_op2_12(vec_op2[12]),
    .io_op2_13(vec_op2[13]),
    .io_op2_14(vec_op2[14]),
    .io_op2_15(vec_op2[15]),
    .io_res_0(vec_res[0]),
    .io_res_1(vec_res[1]),
    .io_res_2(vec_res[2]),
    .io_res_3(vec_res[3]),
    .io_res_4(vec_res[4]),
    .io_res_5(vec_res[5]),
    .io_res_6(vec_res[6]),
    .io_res_7(vec_res[7]),
    .io_res_8(vec_res[8]),
    .io_res_9(vec_res[9]),
    .io_res_10(vec_res[10]),
    .io_res_11(vec_res[11]),
    .io_res_12(vec_res[12]),
    .io_res_13(vec_res[13]),
    .io_res_14(vec_res[14]),
    .io_res_15(vec_res[15]),
    .io_valid(vec_valid),
    .io_done(vec_done)
  );

  function automatic [63:0] fnv_byte(input [63:0] h, input [7:0] b);
    logic [63:0] x;
    begin
      x = h ^ {56'd0, b};
      fnv_byte = x * 64'h00000100000001b3;
    end
  endfunction

  function automatic int pb_resolve(input int unsigned vb);
    int px;
    begin
      pb_resolve = -1;
      for (px = 0; px < BANK_NUM; px = px + 1)
        if (slot_valid[px] && slot_vbank[px] == vb)
          pb_resolve = px;
    end
  endfunction

  function automatic int signed rd_i32_ij(input int p, input int i, input int j);
    int ba;
    logic [31:0] u;
    begin
      ba = p * BANK_SZ + i * ACC_STR + j * 4;
      u = 32'(bram[ba]) | (32'(bram[ba + 1]) << 8) | (32'(bram[ba + 2]) << 16) |
          (32'(bram[ba + 3]) << 24);
      rd_i32_ij = $signed(u);
    end
  endfunction

  task automatic wr_i32_ij(input int p, input int i, input int j, input int signed vv);
    int ba;
    logic [31:0] u;
    begin
      ba = p * BANK_SZ + i * ACC_STR + j * 4;
      u = vv;
      bram[ba + 0] = u[7:0];
      bram[ba + 1] = u[15:8];
      bram[ba + 2] = u[23:16];
      bram[ba + 3] = u[31:24];
    end
  endtask

  integer ri;
  initial begin
    g_dataflow = 1'b0;
    g_ws_b_valid = 1'b0;
    g_ws_n = 7'h0;
    vec_start = 1'b0;
    vec_iter = 16'h0;
    mul64_op1_p = -1;
    mul64_op2_p = -1;
    mul64_wr_p = -1;
    mul64_kin = 0;
    mul64_kk = 0;
    mul64_row = 0;
    mul64_busy = 1'b0;
    for (ri = 0; ri < 16; ri = ri + 1) begin
      vec_op1[ri] = 8'h0;
      vec_op2[ri] = 8'h0;
    end
    for (ri = 0; ri < 4096; ri = ri + 1)
      ws_b_store[ri] = 8'h0;
    for (ri = 0; ri < BANK_NUM * BANK_SZ; ri = ri + 1)
      bram[ri] = 8'h0;
    for (ri = 0; ri < BANK_NUM; ri = ri + 1) begin
      slot_valid[ri] = 1'b0;
      slot_vbank[ri] = 32'h0;
      cfg_alloc[ri] = 1'b0;
      cfg_cols[ri] = 64'h0;
    end
  end

  always_ff @(posedge clk) begin
    int jj;
    integer signed acc;
    vec_start = 1'b0;
    if (mul64_busy) begin
      if (vec_valid) begin
        for (jj = 0; jj < 16; jj = jj + 1) begin
          acc = rd_i32_ij(mul64_wr_p, mul64_row, jj);
          acc = acc + $signed(vec_res[jj]);
          wr_i32_ij(mul64_wr_p, mul64_row, jj, acc);
        end
        mul64_row = mul64_row + 1;
      end
      if (vec_done) begin
        int ii;
        mul64_kk = mul64_kk + 1;
        mul64_row = 0;
        if (mul64_kk >= mul64_kin) begin
          mul64_busy = 1'b0;
        end else begin
          for (ii = 0; ii < 16; ii = ii + 1) begin
            vec_op1[ii] = bram[mul64_op1_p * BANK_SZ + mul64_kk * I8_STR + ii];
            vec_op2[ii] = bram[mul64_op2_p * BANK_SZ + mul64_kk * I8_STR + ii];
          end
          vec_start = 1'b1;
        end
      end
    end
    if (issue_start) begin
    if (funct == 7'd2) begin
      g_dataflow = xs2[4];
    end else if (funct == 7'd32) begin
      int unsigned v;
      int unsigned col;
      bit alloc;
      int pi;
      int free_p;
      int qq;
      v = 32'(xs1[9:0]);
      col = 32'(xs2[9:5]);
      alloc = xs2[10];
      if (v >= BANK_NUM)
        $fatal(1, "bebop_cosim_banks: mset bad vbank");
      for (pi = 0; pi < BANK_NUM; pi = pi + 1) begin
        if (slot_valid[pi] && slot_vbank[pi] == v)
          slot_valid[pi] = 1'b0;
      end
      if (alloc) begin
        free_p = -1;
        for (pi = 0; pi < BANK_NUM; pi = pi + 1) begin
          if (!slot_valid[pi] && free_p < 0)
            free_p = pi;
        end
        if (free_p < 0)
          $fatal(1, "bebop_cosim_banks: mset no free pbank");
        slot_valid[free_p] = 1'b1;
        slot_vbank[free_p] = v;
        cfg_alloc[v] = 1'b1;
        cfg_cols[v] = 64'(col);
        for (qq = 0; qq < BANK_SZ; qq = qq + 1)
          bram[free_p * BANK_SZ + qq] = 8'h0;
      end else begin
        cfg_alloc[v] = 1'b0;
        cfg_cols[v] = 64'h0;
      end
    end else if (funct == 7'd16) begin
      int unsigned bi;
      longint unsigned depth;
      longint unsigned mem_addr;
      longint unsigned stride;
      longint unsigned actual_stride;
      int p;
      longint unsigned rows;
      longint unsigned cols;
      int unsigned line_blocks;
      int unsigned line_bytes;
      longint unsigned i;
      int unsigned b;
      longint unsigned addr_row;
      longint unsigned addr_blk;
      longint unsigned bank_off;
      longint unsigned lo;
      longint unsigned hi;
      int unsigned kk;
      int unsigned idx;
      bi = 32'(xs1[9:0]);
      depth = 64'(xs1[63:30]);
      mem_addr = 64'(xs2[38:0]);
      stride = 64'(xs2[58:39]);
      if (bi >= BANK_NUM)
        $fatal(1, "bebop_cosim_banks: mvout bad bank_id");
      if (!cfg_alloc[bi])
        $fatal(1, "bebop_cosim_banks: mvout bank not allocated");
      p = pb_resolve(bi);
      if (p < 0)
        $fatal(1, "bebop_cosim_banks: mvout pbank miss");
      cols = cfg_cols[bi];
      line_blocks = (cols == 64'h0) ? 32'd1 : 32'(cols[4:0]);
      line_bytes = int'(line_blocks) * 16;
      actual_stride = (stride == 0) ? 1 : stride;
      rows = depth;
      for (i = 0; i < rows; i = i + 1) begin
        addr_row = mem_addr + i * 16 * actual_stride * line_blocks;
        bank_off = i * line_bytes;
        if (bank_off + 64'(line_bytes) > 64'(BANK_SZ))
          $fatal(1, "bebop_cosim_banks: mvout bank range");
        for (b = 0; b < line_blocks; b = b + 1) begin
          int unsigned bb;
          addr_blk = addr_row + 64'(b) * 16;
          bb = b;
          lo = 64'h0;
          hi = 64'h0;
          for (kk = 0; kk < 8; kk = kk + 1) begin
            idx = int'(p) * BANK_SZ + int'(bank_off) + bb * 16 + kk;
            lo |= 64'(bram[idx]) << (8 * int'(kk));
          end
          for (kk = 0; kk < 8; kk = kk + 1) begin
            idx = int'(p) * BANK_SZ + int'(bank_off) + bb * 16 + 8 + kk;
            hi |= 64'(bram[idx]) << (8 * int'(kk));
          end
          dpi_mem_write16(addr_blk, lo, hi);
        end
      end
    end else if (funct == 7'd33) begin
      int unsigned bi;
      longint unsigned depth;
      longint unsigned mem_addr;
      longint unsigned stride;
      longint unsigned actual_stride;
      int p;
      int pi;
      longint unsigned rows;
      longint unsigned cols;
      int unsigned line_blocks;
      int unsigned line_bytes;
      longint unsigned i;
      int unsigned b;
      longint unsigned addr_row;
      longint unsigned addr_blk;
      longint unsigned bank_off;
      longint unsigned lo;
      longint unsigned hi;
      bi = 32'(xs1[9:0]);
      depth = 64'(xs1[63:30]);
      mem_addr = 64'(xs2[38:0]);
      stride = 64'(xs2[58:39]);
      if (bi >= BANK_NUM)
        $fatal(1, "bebop_cosim_banks: mvin bad bank_id");
      if (!cfg_alloc[bi])
        $fatal(1, "bebop_cosim_banks: mvin bank not allocated");
      p = pb_resolve(bi);
      if (p < 0)
        $fatal(1, "bebop_cosim_banks: mvin pbank miss");
      cols = cfg_cols[bi];
      line_blocks = (cols == 64'h0) ? 32'd1 : 32'(cols[4:0]);
      line_bytes = int'(line_blocks) * 16;
      actual_stride = (stride == 0) ? 1 : stride;
      rows = depth;
      for (i = 0; i < rows; i = i + 1) begin
        addr_row = mem_addr + i * 16 * actual_stride * line_blocks;
        bank_off = i * line_bytes;
        if (bank_off + 64'(line_bytes) > 64'(BANK_SZ))
          $fatal(1, "bebop_cosim_banks: mvin bank range");
        for (b = 0; b < line_blocks; b = b + 1) begin
          int unsigned bb;
          int unsigned kk;
          int unsigned idx;
          addr_blk = addr_row + 64'(b) * 16;
          dpi_mem_read16(addr_blk, lo, hi);
          for (kk = 0; kk < 8; kk = kk + 1) begin
            bb = b;
            idx = int'(p) * BANK_SZ + int'(bank_off) + bb * 16 + kk;
            bram[idx] = lo[8*int'(kk) +: 8];
          end
          for (kk = 0; kk < 8; kk = kk + 1) begin
            bb = b;
            idx = int'(p) * BANK_SZ + int'(bank_off) + bb * 16 + 8 + kk;
            bram[idx] = hi[8*int'(kk) +: 8];
          end
        end
      end
    end else if (funct == 7'd48) begin
      int unsigned op1;
      int unsigned wr;
      int po;
      int pw;
      int kcol;
      int krow;
      int incol;
      int inrow;
      int startcol;
      int startrow;
      int row_end;
      int col_end;
      int r;
      int c;
      int kr;
      int kc;
      int src;
      int out_ix;
      op1 = 32'(xs1[9:0]);
      wr = 32'(xs1[29:20]);
      kcol = int'(xs2[3:0]);
      krow = int'(xs2[7:4]);
      incol = int'(xs2[12:8]);
      inrow = int'(xs2[22:13]);
      startcol = int'(xs2[27:23]);
      startrow = int'(xs2[37:28]);
      if (op1 >= BANK_NUM || wr >= BANK_NUM)
        $fatal(1, "bebop_cosim_banks: im2col bank");
      if (!cfg_alloc[op1] || !cfg_alloc[wr])
        $fatal(1, "bebop_cosim_banks: im2col alloc");
      if (op1 == wr)
        $fatal(1, "bebop_cosim_banks: im2col same bank");
      if (kcol == 0 || krow == 0 || incol == 0 || inrow == 0)
        $fatal(1, "bebop_cosim_banks: im2col zero dim");
      if (incol < kcol || inrow < krow)
        $fatal(1, "bebop_cosim_banks: im2col kernel");
      row_end = inrow - krow;
      col_end = incol - kcol;
      if (startrow > row_end || startcol > col_end)
        $fatal(1, "bebop_cosim_banks: im2col start");
      po = pb_resolve(op1);
      pw = pb_resolve(wr);
      if (po < 0 || pw < 0)
        $fatal(1, "bebop_cosim_banks: im2col pbank");
      out_ix = 0;
      for (r = startrow; r <= row_end; r = r + 1) begin
        for (c = startcol; c <= col_end; c = c + 1) begin
          for (kr = 0; kr < krow; kr = kr + 1) begin
            for (kc = 0; kc < kcol; kc = kc + 1) begin
              src = r * incol + c + kr * incol + kc;
              if (src >= BANK_SZ || out_ix >= BANK_SZ)
                $fatal(1, "bebop_cosim_banks: im2col range");
              bram[pw * BANK_SZ + out_ix] = bram[po * BANK_SZ + src];
              out_ix = out_ix + 1;
            end
          end
        end
      end
    end else if (funct == 7'd50) begin
      int unsigned src;
      int unsigned dst;
      int depth;
      int ps;
      int pd;
      int di;
      int dj;
      int base;
      int off;
      logic [31:0] vv;
      logic [31:0] oo;
      src = 32'(xs1[9:0]);
      dst = 32'(xs1[29:20]);
      depth = int'(xs1[63:30]);
      if (src >= BANK_NUM || dst >= BANK_NUM)
        $fatal(1, "bebop_cosim_banks: relu bank");
      if (!cfg_alloc[src] || !cfg_alloc[dst])
        $fatal(1, "bebop_cosim_banks: relu alloc");
      ps = pb_resolve(src);
      pd = pb_resolve(dst);
      if (ps < 0 || pd < 0)
        $fatal(1, "bebop_cosim_banks: relu pbank");
      if (cfg_cols[src] == 64'h1 && cfg_cols[dst] == 64'h1) begin
        for (di = 0; di < depth; di = di + 1) begin
          base = di * 16;
          if (base + 16 > BANK_SZ)
            $fatal(1, "bebop_cosim_banks: relu range");
          for (dj = 0; dj < 16; dj = dj + 1) begin
            bram[pd * BANK_SZ + base + dj] =
              ($signed({24'h0, bram[ps * BANK_SZ + base + dj]}) < 0)
                ? 8'h0
                : bram[ps * BANK_SZ + base + dj];
          end
        end
      end else if (cfg_cols[src] == 64'h4 && cfg_cols[dst] == 64'h4) begin
        for (di = 0; di < depth; di = di + 1) begin
          base = di * 64;
          if (base + 64 > BANK_SZ)
            $fatal(1, "bebop_cosim_banks: relu range");
          for (dj = 0; dj < 16; dj = dj + 1) begin
            off = base + dj * 4;
            vv = 32'(bram[ps * BANK_SZ + off]) |
                (32'(bram[ps * BANK_SZ + off + 1]) << 8) |
                (32'(bram[ps * BANK_SZ + off + 2]) << 16) |
                (32'(bram[ps * BANK_SZ + off + 3]) << 24);
            oo = ($signed(vv) < 0) ? 32'h0 : vv;
            bram[pd * BANK_SZ + off + 0] = oo[7:0];
            bram[pd * BANK_SZ + off + 1] = oo[15:8];
            bram[pd * BANK_SZ + off + 2] = oo[23:16];
            bram[pd * BANK_SZ + off + 3] = oo[31:24];
          end
        end
      end else begin
        $fatal(1, "bebop_cosim_banks: relu layout");
      end
    end else if (funct == 7'd51) begin
      int unsigned src;
      int unsigned dst;
      int depth;
      int ps;
      int pd;
      int di;
      int dj;
      int src_base;
      int dst_base;
      int off;
      int unsigned sb;
      byte unsigned qv;
      src = 32'(xs1[9:0]);
      dst = 32'(xs1[29:20]);
      depth = int'(xs1[63:30]);
      sb = xs2[31:0];
      if (src >= BANK_NUM || dst >= BANK_NUM)
        $fatal(1, "bebop_cosim_banks: quant bank");
      if (!cfg_alloc[src] || !cfg_alloc[dst])
        $fatal(1, "bebop_cosim_banks: quant alloc");
      if (cfg_cols[src] != 64'h4 || cfg_cols[dst] != 64'h1)
        $fatal(1, "bebop_cosim_banks: quant layout");
      ps = pb_resolve(src);
      pd = pb_resolve(dst);
      if (ps < 0 || pd < 0)
        $fatal(1, "bebop_cosim_banks: quant pbank");
      for (di = 0; di < depth; di = di + 1) begin
        src_base = di * 64;
        dst_base = di * 16;
        if (src_base + 64 > BANK_SZ || dst_base + 16 > BANK_SZ)
          $fatal(1, "bebop_cosim_banks: quant range");
        for (dj = 0; dj < 16; dj = dj + 1) begin
          off = src_base + dj * 4;
          qv = bebop_dpi_quant_u8(
            bram[ps * BANK_SZ + off + 0],
            bram[ps * BANK_SZ + off + 1],
            bram[ps * BANK_SZ + off + 2],
            bram[ps * BANK_SZ + off + 3],
            sb
          );
          bram[pd * BANK_SZ + dst_base + dj] = qv;
        end
      end
    end else if (funct == 7'd52) begin
      int unsigned src;
      int unsigned dst;
      int depth;
      int ps;
      int pd;
      int di;
      int dj;
      int src_base;
      int dst_base;
      int off;
      int unsigned sb;
      byte unsigned o0, o1, o2, o3;
      src = 32'(xs1[9:0]);
      dst = 32'(xs1[29:20]);
      depth = int'(xs1[63:30]);
      sb = xs2[31:0];
      if (src >= BANK_NUM || dst >= BANK_NUM)
        $fatal(1, "bebop_cosim_banks: dequant bank");
      if (!cfg_alloc[src] || !cfg_alloc[dst])
        $fatal(1, "bebop_cosim_banks: dequant alloc");
      if (cfg_cols[src] != 64'h1 || cfg_cols[dst] != 64'h4)
        $fatal(1, "bebop_cosim_banks: dequant layout");
      ps = pb_resolve(src);
      pd = pb_resolve(dst);
      if (ps < 0 || pd < 0)
        $fatal(1, "bebop_cosim_banks: dequant pbank");
      for (di = 0; di < depth; di = di + 1) begin
        src_base = di * 16;
        dst_base = di * 64;
        if (src_base + 16 > BANK_SZ || dst_base + 64 > BANK_SZ)
          $fatal(1, "bebop_cosim_banks: dequant range");
        for (dj = 0; dj < 16; dj = dj + 1) begin
          bebop_dpi_dequant_i32_le(bram[ps * BANK_SZ + src_base + dj], sb, o0, o1, o2, o3);
          off = dst_base + dj * 4;
          bram[pd * BANK_SZ + off + 0] = o0;
          bram[pd * BANK_SZ + off + 1] = o1;
          bram[pd * BANK_SZ + off + 2] = o2;
          bram[pd * BANK_SZ + off + 3] = o3;
        end
      end
    end else if (funct == 7'd53) begin
      int unsigned op1;
      int unsigned wr;
      int n;
      int p1;
      int pw;
      int ii;
      int jj;
      op1 = 32'(xs1[9:0]);
      wr = 32'(xs1[29:20]);
      n = int'(xs1[63:30]);
      if (op1 >= BANK_NUM || wr >= BANK_NUM)
        $fatal(1, "bebop_cosim_banks: preload bank");
      if (!cfg_alloc[op1] || !cfg_alloc[wr])
        $fatal(1, "bebop_cosim_banks: preload alloc");
      if (n == 0 || n > 64)
        $fatal(1, "bebop_cosim_banks: preload n");
      p1 = pb_resolve(op1);
      pw = pb_resolve(wr);
      if (p1 < 0 || pw < 0)
        $fatal(1, "bebop_cosim_banks: preload pbank");
      if (g_dataflow) begin
        for (ii = 0; ii < n; ii = ii + 1)
          for (jj = 0; jj < n; jj = jj + 1)
            ws_b_store[ii * 64 + jj] = bram[p1 * BANK_SZ + ii * I8_STR + jj];
        g_ws_n = n[6:0];
        g_ws_b_valid = 1'b1;
      end else begin
        for (ii = 0; ii < n; ii = ii + 1)
          for (jj = 0; jj < n; jj = jj + 1)
            wr_i32_ij(pw, ii, jj, $signed({24'h0, bram[p1 * BANK_SZ + ii * I8_STR + jj]}));
      end
    end else if (funct == 7'd64 && !mul64_busy) begin
      int unsigned op1;
      int unsigned op2;
      int unsigned wr;
      longint unsigned iter;
      int p1;
      int p2;
      int pw;
      int kin;
      int ii;
      op1 = 32'(xs1[9:0]);
      op2 = 32'(xs1[19:10]);
      wr = 32'(xs1[29:20]);
      iter = 64'(xs1[63:30]);
      kin = int'(iter);
      if (op1 >= BANK_NUM || op2 >= BANK_NUM || wr >= BANK_NUM)
        $fatal(1, "bebop_cosim_banks: mul_warp16 bank");
      if (cfg_cols[op1] != 64'h1 || cfg_cols[op2] != 64'h1 || cfg_cols[wr] != 64'h4)
        $fatal(1, "bebop_cosim_banks: mul_warp16 layout");
      p1 = pb_resolve(op1);
      p2 = pb_resolve(op2);
      pw = pb_resolve(wr);
      if (p1 < 0 || p2 < 0 || pw < 0)
        $fatal(1, "bebop_cosim_banks: mul_warp16 pbank");
      if (kin == 0 || (kin % 16) != 0)
        $fatal(1, "bebop_cosim_banks: mul_warp16 kin");
      if (kin * 16 > BANK_SZ)
        $fatal(1, "bebop_cosim_banks: mul_warp16 iter");
      mul64_op1_p = p1;
      mul64_op2_p = p2;
      mul64_wr_p = pw;
      mul64_kin = kin;
      mul64_kk = 0;
      mul64_row = 0;
      vec_iter = 16'(kin);
      for (ii = 0; ii < 16; ii = ii + 1) begin
        vec_op1[ii] = bram[p1 * BANK_SZ + ii];
        vec_op2[ii] = bram[p2 * BANK_SZ + ii];
      end
      vec_start = 1'b1;
      mul64_busy = 1'b1;
    end else if (funct == 7'd65) begin
      int unsigned op1;
      int unsigned op2;
      int unsigned wr;
      int n;
      int p1;
      int p2;
      int pw;
      int ii;
      int jj;
      int kk;
      integer signed acc;
      integer signed aa;
      integer signed bb;
      op1 = 32'(xs1[9:0]);
      op2 = 32'(xs1[19:10]);
      wr = 32'(xs1[29:20]);
      n = int'(xs1[63:30]);
      if (op1 >= BANK_NUM || op2 >= BANK_NUM || wr >= BANK_NUM)
        $fatal(1, "bebop_cosim_banks: bfp bank");
      if (!cfg_alloc[op1] || !cfg_alloc[op2] || !cfg_alloc[wr])
        $fatal(1, "bebop_cosim_banks: bfp alloc");
      if (cfg_cols[wr] != 64'h4)
        $fatal(1, "bebop_cosim_banks: bfp wr");
      if (n == 0 || n > 64)
        $fatal(1, "bebop_cosim_banks: bfp n");
      p1 = pb_resolve(op1);
      p2 = pb_resolve(op2);
      pw = pb_resolve(wr);
      if (p1 < 0 || p2 < 0 || pw < 0)
        $fatal(1, "bebop_cosim_banks: bfp pbank");
      for (ii = 0; ii < n; ii = ii + 1) begin
        for (jj = 0; jj < n; jj = jj + 1) begin
          acc = 0;
          for (kk = 0; kk < n; kk = kk + 1) begin
            aa = $signed({24'h0, bram[p1 * BANK_SZ + ii * I8_STR + kk]});
            bb = $signed({24'h0, bram[p2 * BANK_SZ + kk * I8_STR + jj]});
            acc = acc + aa * bb;
          end
          wr_i32_ij(pw, ii, jj, acc);
        end
      end
    end else if (funct == 7'd66) begin
      int unsigned op_a;
      int unsigned op_b;
      int unsigned wr;
      int n;
      int pa;
      int pb_;
      int pw;
      int ii;
      int jj;
      int kk;
      integer signed acc;
      integer signed aa;
      integer signed bb;
      op_a = 32'(xs1[9:0]);
      op_b = 32'(xs1[19:10]);
      wr = 32'(xs1[29:20]);
      n = int'(xs1[63:30]);
      if (op_a >= BANK_NUM || op_b >= BANK_NUM || wr >= BANK_NUM)
        $fatal(1, "bebop_cosim_banks: gcmp pre bank");
      if (!cfg_alloc[op_a] || !cfg_alloc[op_b] || !cfg_alloc[wr])
        $fatal(1, "bebop_cosim_banks: gcmp pre alloc");
      if (n == 0 || n > 64)
        $fatal(1, "bebop_cosim_banks: gcmp pre n");
      pa = pb_resolve(op_a);
      pb_ = pb_resolve(op_b);
      pw = pb_resolve(wr);
      if (pa < 0 || pb_ < 0 || pw < 0)
        $fatal(1, "bebop_cosim_banks: gcmp pre pbank");
      if (g_dataflow) begin
        if (!g_ws_b_valid)
          $fatal(1, "bebop_cosim_banks: gcmp pre ws_b");
        for (ii = 0; ii < n; ii = ii + 1) begin
          for (jj = 0; jj < n; jj = jj + 1) begin
            acc = rd_i32_ij(pb_, ii, jj);
            for (kk = 0; kk < n; kk = kk + 1) begin
              aa = $signed({24'h0, bram[pa * BANK_SZ + ii * I8_STR + kk]});
              bb = $signed({24'h0, ws_b_store[kk * 64 + jj]});
              acc = acc + aa * bb;
            end
            wr_i32_ij(pw, ii, jj, acc);
          end
        end
      end else begin
        for (ii = 0; ii < n; ii = ii + 1) begin
          for (jj = 0; jj < n; jj = jj + 1) begin
            acc = rd_i32_ij(pw, ii, jj);
            for (kk = 0; kk < n; kk = kk + 1) begin
              aa = $signed({24'h0, bram[pa * BANK_SZ + kk * I8_STR + ii]});
              bb = $signed({24'h0, bram[pb_ * BANK_SZ + kk * I8_STR + jj]});
              acc = acc + aa * bb;
            end
            wr_i32_ij(pw, ii, jj, acc);
          end
        end
      end
    end else if (funct == 7'd67) begin
      int unsigned op_a;
      int unsigned op_b;
      int unsigned wr;
      int n;
      int pa;
      int pb_;
      int pw;
      int ii;
      int jj;
      int kk;
      integer signed acc;
      integer signed aa;
      integer signed bb;
      op_a = 32'(xs1[9:0]);
      op_b = 32'(xs1[19:10]);
      wr = 32'(xs1[29:20]);
      n = int'(xs1[63:30]);
      if (op_a >= BANK_NUM || op_b >= BANK_NUM || wr >= BANK_NUM)
        $fatal(1, "bebop_cosim_banks: gcmp acc bank");
      if (!cfg_alloc[op_a] || !cfg_alloc[op_b] || !cfg_alloc[wr])
        $fatal(1, "bebop_cosim_banks: gcmp acc alloc");
      if (n == 0 || n > 64)
        $fatal(1, "bebop_cosim_banks: gcmp acc n");
      pa = pb_resolve(op_a);
      pb_ = pb_resolve(op_b);
      pw = pb_resolve(wr);
      if (pa < 0 || pb_ < 0 || pw < 0)
        $fatal(1, "bebop_cosim_banks: gcmp acc pbank");
      for (ii = 0; ii < n; ii = ii + 1) begin
        for (jj = 0; jj < n; jj = jj + 1) begin
          acc = rd_i32_ij(pw, ii, jj);
          for (kk = 0; kk < n; kk = kk + 1) begin
            aa = $signed({24'h0, bram[pa * BANK_SZ + kk * I8_STR + ii]});
            bb = $signed({24'h0, bram[pb_ * BANK_SZ + kk * I8_STR + jj]});
            acc = acc + aa * bb;
          end
          wr_i32_ij(pw, ii, jj, acc);
        end
      end
    end else if (funct == 7'd49) begin
      int unsigned op1;
      int unsigned wr;
      longint unsigned iter;
      int po;
      int pw;
      longint unsigned c1;
      longint unsigned cw;
      int pi;
      int r;
      int c;
      longint unsigned src_ix;
      longint unsigned dst_ix;
      int n;
      int i;
      int j;
      longint unsigned src_off;
      longint unsigned dst_off;
      logic [31:0] w32;
      op1 = 32'(xs1[9:0]);
      wr = 32'(xs1[29:20]);
      iter = 64'(xs1[63:30]);
      if (op1 >= BANK_NUM || wr >= BANK_NUM)
        $fatal(1, "bebop_cosim_banks: transpose bad bank");
      if (!cfg_alloc[op1] || !cfg_alloc[wr])
        $fatal(1, "bebop_cosim_banks: transpose bad alloc");
      c1 = cfg_cols[op1];
      cw = cfg_cols[wr];
      po = pb_resolve(op1);
      pw = pb_resolve(wr);
      if (po < 0 || pw < 0)
        $fatal(1, "bebop_cosim_banks: transpose pbank");
      if (c1 == 64'h1 && cw == 64'h1) begin
        if (iter == 0)
          $fatal(1, "bebop_cosim_banks: transpose iter");
        if (po == pw)
          $fatal(1, "bebop_cosim_banks: transpose same pbank");
        for (r = 0; r < 16; r = r + 1) begin
          for (c = 0; 64'(c) < iter; c = c + 1) begin
            src_ix = 64'(r) * iter + 64'(c);
            dst_ix = 64'(c) * 16 + 64'(r);
            if (src_ix >= 64'(BANK_SZ) || dst_ix >= 64'(BANK_SZ))
              $fatal(1, "bebop_cosim_banks: transpose range");
            bram[pw * BANK_SZ + int'(dst_ix)] = bram[po * BANK_SZ + int'(src_ix)];
          end
        end
      end else if (c1 == 64'h4 && cw == 64'h4) begin
        n = (iter < 16) ? int'(iter) : 16;
        for (i = 0; i < n; i = i + 1) begin
          for (j = 0; j < n; j = j + 1) begin
            src_off = 64'(i) * 64 + 64'(j) * 4;
            dst_off = 64'(j) * 64 + 64'(i) * 4;
            w32 = 32'(bram[po * BANK_SZ + int'(src_off) + 0]) |
                (32'(bram[po * BANK_SZ + int'(src_off) + 1]) << 8) |
                (32'(bram[po * BANK_SZ + int'(src_off) + 2]) << 16) |
                (32'(bram[po * BANK_SZ + int'(src_off) + 3]) << 24);
            bram[pw * BANK_SZ + int'(dst_off) + 0] = w32[7:0];
            bram[pw * BANK_SZ + int'(dst_off) + 1] = w32[15:8];
            bram[pw * BANK_SZ + int'(dst_off) + 2] = w32[23:16];
            bram[pw * BANK_SZ + int'(dst_off) + 3] = w32[31:24];
          end
        end
      end else begin
        $fatal(1, "bebop_cosim_banks: transpose unsupported");
      end
    end
    end
  end

  always_comb begin
    logic [63:0] h;
    bit any;
    int ii;
    int jj;
    int kk;
    h = 64'hcbf29ce484222325;
    any = 0;
    for (ii = 0; ii < BANK_NUM; ii = ii + 1) begin
      if (digest_all_banks || cfg_alloc[ii]) begin
        any = 1;
        for (kk = 0; kk < 4; kk = kk + 1)
          h = fnv_byte(h, 8'(ii >> (kk * 8)));
        for (kk = 0; kk < 4; kk = kk + 1)
          h = fnv_byte(h, 8'(BANK_SZ >> (kk * 8)));
        for (jj = 0; jj < BANK_SZ; jj = jj + 1)
          h = fnv_byte(h, bram[ii * BANK_SZ + jj]);
      end
    end
    if (!any)
      bank_digest_peek = 64'h0;
    else
      bank_digest_peek = h;
  end

  assign banks_busy = mul64_busy;
endmodule
