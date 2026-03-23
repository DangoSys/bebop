//! Bank rename scoreboard experiment: NoRename vs WriteAlias (BAT-style).

use std::path::PathBuf;

use crate::emu::bank::BANK_NUM;
use crate::emu::inst::decode::{
    self, rs1_b0, rs1_b1, rs1_b2, FUNCT_BFP, FUNCT_DEQUANT, FUNCT_GEMMINI_COMPUTE_ACCUMULATED,
    FUNCT_GEMMINI_COMPUTE_PRELOADED, FUNCT_GEMMINI_PRELOAD, FUNCT_IM2COL, FUNCT_MUL_WARP16,
    FUNCT_MVIN, FUNCT_MVOUT, FUNCT_QUANT, FUNCT_RELU, FUNCT_TRANSPOSE,
};
use crate::emu::inst::exec_latency::cycles_after_issue;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct VbankAccess {
    rd0: Option<u32>,
    rd1: Option<u32>,
    wr: Option<u32>,
}

impl VbankAccess {
    fn has_port(&self) -> bool {
        self.rd0.is_some() || self.rd1.is_some() || self.wr.is_some()
    }
}

fn extract_vbank_access(funct: u32, xs1: u64, _xs2: u64) -> Option<VbankAccess> {
    match funct {
        FUNCT_MVIN => Some(VbankAccess {
            rd0: None,
            rd1: None,
            wr: Some(rs1_b0(xs1) as u32),
        }),
        FUNCT_MVOUT => Some(VbankAccess {
            rd0: Some(rs1_b0(xs1) as u32),
            rd1: None,
            wr: None,
        }),
        FUNCT_IM2COL => Some(VbankAccess {
            rd0: Some(rs1_b0(xs1) as u32),
            rd1: None,
            wr: Some(rs1_b2(xs1) as u32),
        }),
        FUNCT_TRANSPOSE => Some(VbankAccess {
            rd0: Some(rs1_b0(xs1) as u32),
            rd1: None,
            wr: Some(rs1_b2(xs1) as u32),
        }),
        FUNCT_RELU | FUNCT_QUANT | FUNCT_DEQUANT => Some(VbankAccess {
            rd0: Some(rs1_b0(xs1) as u32),
            rd1: None,
            wr: Some(rs1_b2(xs1) as u32),
        }),
        FUNCT_GEMMINI_PRELOAD => Some(VbankAccess {
            rd0: Some(rs1_b0(xs1) as u32),
            rd1: None,
            wr: Some(rs1_b2(xs1) as u32),
        }),
        FUNCT_MUL_WARP16
        | FUNCT_BFP
        | FUNCT_GEMMINI_COMPUTE_PRELOADED
        | FUNCT_GEMMINI_COMPUTE_ACCUMULATED => Some(VbankAccess {
            rd0: Some(rs1_b0(xs1) as u32),
            rd1: Some(rs1_b1(xs1) as u32),
            wr: Some(rs1_b2(xs1) as u32),
        }),
        f if f == decode::FUNCT_FENCE
            || f == decode::FUNCT_BARRIER
            || f == decode::FUNCT_GEMMINI_CONFIG
            || f == decode::FUNCT_GEMMINI_FLUSH
            || f == decode::FUNCT_BDB_COUNTER
            || f == decode::FUNCT_BDB_BACKDOOR
            || f == decode::FUNCT_GEMMINI_LOOP_WS_CONFIG_BOUNDS
            || (f >= decode::FUNCT_GEMMINI_LOOP_WS_CONFIG_ADDR_A
                && f <= decode::FUNCT_GEMMINI_LOOP_WS_CONFIG_STRIDES_DC)
            || f == decode::FUNCT_GEMMINI_LOOP_WS
            || (f >= decode::FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_1
                && f <= decode::FUNCT_GEMMINI_LOOP_CONV_WS_CONFIG_9)
            || f == decode::FUNCT_GEMMINI_LOOP_CONV_WS =>
        {
            None
        }
        _ => None,
    }
}

const MAX_SB: usize = 1024;

#[derive(Clone, Copy, Debug)]
pub struct Scoreboard {
    pub rd_cnt: [u32; MAX_SB],
    pub wr_busy: [bool; MAX_SB],
}

impl Default for Scoreboard {
    fn default() -> Self {
        Scoreboard {
            rd_cnt: [0u32; MAX_SB],
            wr_busy: [false; MAX_SB],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StallKind {
    Raw,
    War,
    Waw,
}

#[derive(Clone, Debug, Default)]
pub struct RenamedAccess {
    pub rd0: Option<u32>,
    pub rd1: Option<u32>,
    pub wr: Option<u32>,
}

impl RenamedAccess {
    fn from_vbank_no_rename(v: &VbankAccess) -> Self {
        RenamedAccess {
            rd0: v.rd0,
            rd1: v.rd1,
            wr: v.wr,
        }
    }

    fn plan_write_alias(
        v: &VbankAccess,
        v2a: &[u32; BANK_NUM],
        alias_next: u32,
    ) -> (Self, Option<WriteRestore>) {
        let ren = RenamedAccess {
            rd0: v.rd0.map(|vb| v2a[vb as usize]),
            rd1: v.rd1.map(|vb| v2a[vb as usize]),
            wr: v.wr.map(|_| alias_next),
        };
        let wr = v.wr.map(|vb| WriteRestore {
            vbank: vb,
            alias: alias_next,
            old_v2a: v2a[vb as usize],
        });
        (ren, wr)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WriteRestore {
    pub vbank: u32,
    pub alias: u32,
    pub old_v2a: u32,
}

fn hazard_kind(sb: &Scoreboard, acc: &RenamedAccess) -> Option<StallKind> {
    if let Some(id) = acc.rd0 {
        let i = id as usize;
        if i >= MAX_SB {
            panic!("hazard_kind: rd0 id {}", id);
        }
        if sb.wr_busy[i] {
            return Some(StallKind::Raw);
        }
    }
    if let Some(id) = acc.rd1 {
        let i = id as usize;
        if i >= MAX_SB {
            panic!("hazard_kind: rd1 id {}", id);
        }
        if sb.wr_busy[i] {
            return Some(StallKind::Raw);
        }
    }
    if let Some(id) = acc.wr {
        let i = id as usize;
        if i >= MAX_SB {
            panic!("hazard_kind: wr id {}", id);
        }
        if sb.rd_cnt[i] != 0 {
            return Some(StallKind::War);
        }
        if sb.wr_busy[i] {
            return Some(StallKind::Waw);
        }
    }
    None
}

fn apply_issue(sb: &mut Scoreboard, acc: &RenamedAccess) {
    if let Some(id) = acc.rd0 {
        sb.rd_cnt[id as usize] += 1;
    }
    if let Some(id) = acc.rd1 {
        sb.rd_cnt[id as usize] += 1;
    }
    if let Some(id) = acc.wr {
        sb.wr_busy[id as usize] = true;
    }
}

fn apply_complete(sb: &mut Scoreboard, acc: &RenamedAccess) {
    if let Some(id) = acc.rd0 {
        sb.rd_cnt[id as usize] = sb.rd_cnt[id as usize].saturating_sub(1);
    }
    if let Some(id) = acc.rd1 {
        sb.rd_cnt[id as usize] = sb.rd_cnt[id as usize].saturating_sub(1);
    }
    if let Some(id) = acc.wr {
        sb.wr_busy[id as usize] = false;
    }
}

#[derive(Debug, Clone)]
pub struct Inflight {
    pub complete_at: u64,
    pub acc: RenamedAccess,
    pub wr_restore: Option<WriteRestore>,
}

#[derive(Debug, Default, Clone)]
pub struct SimMetrics {
    pub cycles: u64,
    pub issued: u64,
    pub stall_cycles: u64,
    pub raw_stalls: u64,
    pub war_stalls: u64,
    pub waw_stalls: u64,
}

#[derive(Copy, Clone, Debug)]
pub enum RenameMode {
    NoRename,
    WriteAlias,
}

pub fn run_sim(trace: &[(u32, u64, u64)], latency: &[u64], mode: RenameMode) -> SimMetrics {
    assert_eq!(
        trace.len(),
        latency.len(),
        "trace and latency length mismatch"
    );

    let waiting = trace.to_vec();
    let mut inflight: Vec<Inflight> = Vec::new();
    let mut sb = Scoreboard::default();
    let mut v2a: [u32; BANK_NUM] = std::array::from_fn(|i| i as u32);
    let mut alias_next: u32 = BANK_NUM as u32;

    let mut m = SimMetrics::default();
    let mut ip: usize = 0;

    while ip < waiting.len() || !inflight.is_empty() {
        m.cycles += 1;

        let mut idx = 0;
        while idx < inflight.len() {
            if inflight[idx].complete_at == m.cycles {
                let ent = inflight.remove(idx);
                apply_complete(&mut sb, &ent.acc);
                if let Some(w) = ent.wr_restore {
                    if v2a[w.vbank as usize] == w.alias {
                        v2a[w.vbank as usize] = w.old_v2a;
                    }
                }
            } else {
                idx += 1;
            }
        }

        if ip >= waiting.len() {
            continue;
        }

        let (funct, xs1, xs2) = waiting[ip];
        let v_acc = extract_vbank_access(funct, xs1, xs2);

        let (ren, wr_restore) = match v_acc {
            None => (RenamedAccess::default(), None),
            Some(ref v) => match mode {
                RenameMode::NoRename => (RenamedAccess::from_vbank_no_rename(v), None),
                RenameMode::WriteAlias => RenamedAccess::plan_write_alias(v, &v2a, alias_next),
            },
        };

        let lat_cycles = latency[ip];
        let has_port = v_acc.as_ref().map(|x| x.has_port()).unwrap_or(false);

        if !has_port {
            apply_issue(&mut sb, &ren);
            if let Some(ref w) = wr_restore {
                v2a[w.vbank as usize] = w.alias;
                alias_next += 1;
                if alias_next as usize >= MAX_SB {
                    panic!("experiment::bank_rename: alias id overflow MAX_SB");
                }
            }
            inflight.push(Inflight {
                complete_at: m.cycles + lat_cycles,
                acc: ren,
                wr_restore,
            });
            m.issued += 1;
            ip += 1;
            continue;
        }

        if let Some(k) = hazard_kind(&sb, &ren) {
            m.stall_cycles += 1;
            match k {
                StallKind::Raw => m.raw_stalls += 1,
                StallKind::War => m.war_stalls += 1,
                StallKind::Waw => m.waw_stalls += 1,
            }
            continue;
        }

        apply_issue(&mut sb, &ren);
        if let Some(ref w) = wr_restore {
            v2a[w.vbank as usize] = w.alias;
            alias_next += 1;
            if alias_next as usize >= MAX_SB {
                panic!("experiment::bank_rename: alias id overflow MAX_SB");
            }
        }
        inflight.push(Inflight {
            complete_at: m.cycles + lat_cycles,
            acc: ren,
            wr_restore,
        });
        m.issued += 1;
        ip += 1;
    }

    m
}

#[derive(Clone, Debug)]
pub struct RoccInsn {
    pub funct: u32,
    pub xs1: u64,
    pub xs2: u64,
    pub lat_from_log: Option<u64>,
}

pub fn parse_rocc_step_lines_detailed(text: &str) -> Vec<RoccInsn> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with("step=") {
            continue;
        }
        let Some(rest) = line.strip_prefix("step=") else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let _ = parts.next();

        let mut funct: Option<u32> = None;
        let mut xs1: Option<u64> = None;
        let mut xs2: Option<u64> = None;
        let mut lat_from_log: Option<u64> = None;
        for p in parts {
            if let Some(v) = p.strip_prefix("funct=") {
                funct = v.parse().ok();
            } else if let Some(v) = p.strip_prefix("xs1=0x") {
                xs1 = u64::from_str_radix(v, 16).ok();
            } else if let Some(v) = p.strip_prefix("xs2=0x") {
                xs2 = u64::from_str_radix(v, 16).ok();
            } else if let Some(v) = p.strip_prefix("lat=") {
                lat_from_log = v.parse().ok();
            }
        }
        if let (Some(f), Some(x1), Some(x2)) = (funct, xs1, xs2) {
            out.push(RoccInsn {
                funct: f,
                xs1: x1,
                xs2: x2,
                lat_from_log,
            });
        }
    }
    out
}

pub fn latency_vector_for_trace(insns: &[RoccInsn]) -> Vec<u64> {
    insns
        .iter()
        .map(|i| {
            i.lat_from_log
                .unwrap_or_else(|| cycles_after_issue(i.funct, i.xs1, i.xs2))
        })
        .collect()
}

fn print_metrics(label: &str, m: &SimMetrics) {
    let ipc = if m.cycles > 0 {
        m.issued as f64 / m.cycles as f64
    } else {
        0.0
    };
    println!(
        "{}: cycles={} issued={} stall_cycles={} RAW={} WAR={} WAW={} issued_per_cycle={:.4}",
        label, m.cycles, m.issued, m.stall_cycles, m.raw_stalls, m.war_stalls, m.waw_stalls, ipc
    );
}

pub fn run_rocc_step(log: PathBuf, bemu_latency: bool, latency: Vec<u64>) -> Result<(), String> {
    let text = std::fs::read_to_string(&log).map_err(|e| format!("read {}: {e}", log.display()))?;
    let detailed = parse_rocc_step_lines_detailed(&text);
    let trace: Vec<(u32, u64, u64)> = detailed.iter().map(|r| (r.funct, r.xs1, r.xs2)).collect();
    if trace.is_empty() {
        return Err(format!("no RoCC lines parsed from {}", log.display()));
    }
    if bemu_latency {
        let lat = latency_vector_for_trace(&detailed);
        println!(
            "--- bemu issue→complete model (instructions={}) ---",
            trace.len()
        );
        let a = run_sim(&trace, &lat, RenameMode::NoRename);
        let b = run_sim(&trace, &lat, RenameMode::WriteAlias);
        print_metrics("NoRename", &a);
        print_metrics("WriteAlias", &b);
        println!(
            "delta stall_cycles: {} -> {} ({:+})",
            a.stall_cycles,
            b.stall_cycles,
            b.stall_cycles as i64 - a.stall_cycles as i64
        );
    } else {
        for lat_val in &latency {
            println!(
                "--- fixed latency L={} (instructions={}) ---",
                lat_val,
                trace.len()
            );
            let lat = vec![*lat_val; trace.len()];
            let a = run_sim(&trace, &lat, RenameMode::NoRename);
            let b = run_sim(&trace, &lat, RenameMode::WriteAlias);
            print_metrics("NoRename", &a);
            print_metrics("WriteAlias", &b);
            println!(
                "delta stall_cycles: {} -> {} ({:+})",
                a.stall_cycles,
                b.stall_cycles,
                b.stall_cycles as i64 - a.stall_cycles as i64
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emu::inst::decode::FUNCT_MVIN;

    #[test]
    fn rename_reduces_waw_two_mvin_same_vbank() {
        let trace = vec![(FUNCT_MVIN, 0, 0), (FUNCT_MVIN, 0, 0)];
        let lat = vec![2u64, 2];
        let a = run_sim(&trace, &lat, RenameMode::NoRename);
        let b = run_sim(&trace, &lat, RenameMode::WriteAlias);
        assert!(
            b.stall_cycles < a.stall_cycles,
            "expected fewer stalls with rename: a={:?} b={:?}",
            a,
            b
        );
        assert!(a.waw_stalls >= 1);
        assert_eq!(b.waw_stalls, 0);
    }
}
