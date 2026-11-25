#!/usr/bin/env python3
"""Install the design.pb of one simulation using Buckyball's config implementation."""

import argparse
import importlib.util
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--buckyball", type=Path)
    parser.add_argument("--config", required=True)
    parser.add_argument("--kind", required=True)
    parser.add_argument("--name", required=True)
    args = parser.parse_args()
    bebop = Path(__file__).resolve().parent.parent
    repo = args.buckyball or (
        bebop.parent
        if (bebop.parent / "bbdev").is_dir()
        else bebop / "thirdparty/buckyball"
    )
    script = repo / "bbdev/api/steps/config/scripts/install.py"
    spec = importlib.util.spec_from_file_location("config_install", script)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    print(module.install(repo, args.config, args.kind, args.name))


if __name__ == "__main__":
    main()
