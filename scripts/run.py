#!/usr/bin/env python3
"""Run a design's BEMU model using the shared Buckyball log directory convention."""

import argparse
from datetime import datetime
from pathlib import Path
import subprocess
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--buckyball", type=Path, required=True)
    parser.add_argument("--design", required=True)
    parser.add_argument("--selection", required=True)
    parser.add_argument("--elf", type=Path, required=True)
    parser.add_argument("runner_args", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    sys.path.insert(0, str(args.buckyball / "bbdev/api"))
    from utils.path import bebop_bin_path, log_dir

    elf = args.elf.resolve(strict=True)
    log = Path(
        log_dir(
            str(args.buckyball),
            args.design,
            "bemu",
            datetime.now().strftime("%Y%m%d-%H%M%S-%f"),
            "bemu",
            elf.name,
            selection=args.selection,
        )
    )
    log.mkdir(parents=True)
    executable = bebop_bin_path(
        str(args.buckyball), args.design, "bemu", selection=args.selection
    )
    print(f"Log directory: {log}", flush=True)
    result = subprocess.run(
        [executable, "--elf", str(elf), "--log-dir", str(log), *args.runner_args[1:]]
    )
    sys.exit(result.returncode if result.returncode >= 0 else 128 - result.returncode)


if __name__ == "__main__":
    main()
