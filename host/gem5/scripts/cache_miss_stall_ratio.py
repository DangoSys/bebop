#!/usr/bin/env python3
"""
Parse gem5 stats.txt and report cache miss stall as fraction of total cycles.

Stall cycles = time CPU waits for I-cache and D-cache misses (L1 miss latency
in ticks, converted to cycles). Ratio = stall_cycles / numCycles.
"""

import re
import sys


def parse_stats(path):
  with open(path) as f:
    text = f.read()
  # name followed by whitespace and number (first group)
  pat = re.compile(r"^(\S+)\s+(\S+)\s+#", re.MULTILINE)
  stats = {}
  for m in pat.finditer(text):
    name, val = m.group(1), m.group(2)
    if val in ("nan", "inf"):
      continue
    try:
      stats[name] = int(float(val))
    except ValueError:
      try:
        stats[name] = float(val)
      except ValueError:
        pass
  return stats


def main():
  path = sys.argv[1] if len(sys.argv) > 1 else "m5out/stats.txt"
  s = parse_stats(path)

  clock = s.get("system.clk_domain.clock")
  num_cycles = s.get("system.cpu.numCycles")
  icache_miss_ticks = s.get("system.cpu.icache.overallMissLatency::total")
  dcache_miss_ticks = s.get("system.cpu.dcache.overallMissLatency::total")

  if clock is None or num_cycles is None:
    raise SystemExit("Missing system.clk_domain.clock or system.cpu.numCycles")
  if icache_miss_ticks is None:
    icache_miss_ticks = 0
  if dcache_miss_ticks is None:
    dcache_miss_ticks = 0

  icache_stall_cycles = icache_miss_ticks // clock
  dcache_stall_cycles = dcache_miss_ticks // clock
  total_stall_cycles = icache_stall_cycles + dcache_stall_cycles
  ratio = total_stall_cycles / num_cycles if num_cycles else 0

  print("Cache miss stall (from L1 miss latency):")
  print(f"  I-cache miss latency (ticks)  = {icache_miss_ticks}")
  print(f"  D-cache miss latency (ticks)  = {dcache_miss_ticks}")
  print(f"  Clock (ticks/cycle)            = {clock}")
  print(f"  I-cache stall (cycles)        = {icache_stall_cycles}")
  print(f"  D-cache stall (cycles)        = {dcache_stall_cycles}")
  print(f"  Total cache miss stall (cycles) = {total_stall_cycles}")
  print(f"  Total CPU cycles              = {num_cycles}")
  print(f"  Cache miss stall ratio        = {ratio:.2%}")


if __name__ == "__main__":
  main()
