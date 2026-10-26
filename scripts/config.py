#!/usr/bin/env python3
"""Run the Buckyball configuration installation step for a standalone Bebop checkout."""

import argparse
import importlib.util
import json
import subprocess
import sys
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--design", required=True)
    args = parser.parse_args()
    bebop = Path(__file__).resolve().parent.parent
    buckyball = bebop / "thirdparty/buckyball"
    subprocess.run(
        [
            "git",
            "-C",
            str(bebop),
            "submodule",
            "update",
            "--init",
            "thirdparty/buckyball",
        ],
        check=True,
    )
    subprocess.run(
        ["git", "-C", str(buckyball), "submodule", "update", "--init", "bbdev"],
        check=True,
    )
    scripts = buckyball / "bbdev/api/steps/config/scripts"
    sys.path.insert(0, str(scripts))
    subprocess.run(
        [
            "protoc",
            f"-I{scripts / 'proto'}",
            f"--python_out={scripts}",
            str(scripts / "proto/design.proto"),
        ],
        check=True,
    )
    modules = []
    for filename in [
        "1_toml2json.py",
        "2_parameter_derivation.py",
        "3_json2proto.py",
        "4_install.py",
    ]:
        spec = importlib.util.spec_from_file_location(filename, scripts / filename)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        modules.append(module)
    expand, derive, emit, install = modules
    top = buckyball / "examples/designs" / args.design / "configs/design.toml"
    generated = top.parent / "generated"
    config = generated / "config/config.json"
    config.parent.mkdir(parents=True, exist_ok=True)
    data = expand.design2json(top)
    config.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")
    derived = config.with_name("derived.json")
    derive.write_derived(data, buckyball, args.design, derived)
    pb_path = config.with_name("design.pb")
    emit.write_design_pb(config, derived, pb_path, buckyball)
    design = install.load_design(pb_path)
    install.install_arch(pb_path, generated)
    # Keep generated build artifacts in Buckyball; bind their engine paths to this Bebop.
    bemu = generated / "bemu"
    bemu.mkdir(parents=True, exist_ok=True)
    install._emit_dispatch(design, buckyball, bemu / "dispatch.rs")
    template = bebop / "src/nodes/bemu/design"
    manifest = (
        (template / "Cargo.toml")
        .read_text()
        .replace("../../../../../../bebop/", str(bebop) + "/")
    )
    (bemu / "Cargo.toml").write_text(manifest)
    build = bemu / "build.rs"
    build.unlink(missing_ok=True)
    build.write_text(
        (template / "build.rs")
        .read_text()
        .replace("../../../../../../bbdev/", str(buckyball / "bbdev") + "/")
        .replace("../../../../../../bebop/", str(bebop) + "/")
    )
    install.install_bebop(generated, bebop)
    install.install_workload(design, buckyball, args.design, generated)
    print(f"Installed design configuration: {generated / 'design.pb'}")


if __name__ == "__main__":
    main()
