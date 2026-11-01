#!/usr/bin/env python3
"""Generate a selected simulation's design.pb using Buckyball's config implementation."""

import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--buckyball", type=Path)
    parser.add_argument("--design", required=True)
    parser.add_argument("--selection")
    args = parser.parse_args()
    bebop = Path(__file__).resolve().parent.parent
    repo = args.buckyball or (
        bebop.parent
        if (bebop.parent / "bbdev").is_dir()
        else bebop / "thirdparty/buckyball"
    )
    repo = repo.resolve()
    scripts = repo / "bbdev/api/steps/config/scripts"
    sys.path.insert(0, str(scripts))
    modules = []
    with tempfile.TemporaryDirectory(prefix="bebop-proto-") as generated:
        schemas = [scripts / "proto/design.proto", scripts / "proto/hardware.proto"]
        schemas += sorted(repo.glob("examples/prefab/balls/*/configs/*_params.proto"))
        schemas += sorted(repo.glob("examples/prefab/cores/*/configs/*_params.proto"))
        subprocess.run(
            [
                "protoc",
                *[f"-I{p}" for p in sorted({p.parent for p in schemas})],
                f"--python_out={generated}",
                *map(str, schemas),
            ],
            check=True,
        )
        sys.path.insert(0, generated)
        for name in ["1_toml2json.py", "2_parameter_derivation.py", "3_json2proto.py"]:
            spec = importlib.util.spec_from_file_location(name, scripts / name)
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            modules.append(module)
        expand, derive, emit = modules
        design = repo / "examples/designs" / args.design
        data = expand.design2json(design / "configs/design.toml")
        matches = [
            s
            for s in expand.simulations(data)
            if args.selection is None or s[2] == args.selection
        ]
        if not matches:
            raise ValueError(f"Unknown simulation: {args.selection}")
        for simulation in matches:
            selected = expand.simulation2json(data, *simulation)
            output = design / "output/sims" / simulation[2] / "generated"
            config = output / "config/config.json"
            config.parent.mkdir(parents=True, exist_ok=True)
            config.write_text(json.dumps(selected, indent=2, sort_keys=True) + "\n")
            derived = config.with_name("derived.json")
            derive.write_derived(selected, repo, args.design, derived)
            emit.write_design_pb(config, derived, output / "design.pb", repo)
            print(f"Generated {output / 'design.pb'}")


if __name__ == "__main__":
    main()
