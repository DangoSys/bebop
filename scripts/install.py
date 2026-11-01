#!/usr/bin/env python3
"""Install the BEMU command for a library or standalone Bebop checkout."""

import os
from pathlib import Path
import re
import shutil
import subprocess


def main():
    bebop = Path(__file__).resolve().parent.parent
    if (bebop.parent / "bbdev").is_dir():
        repo = bebop.parent
    else:
        repo = bebop / "thirdparty/buckyball"
        if not (repo / ".git").exists():
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
        # Source manifests in Buckyball point to its Bebop submodule in library
        # mode. Bind those paths to this checkout; do not initialize another Bebop.
        files = subprocess.check_output(["git", "-C", str(repo), "ls-files", "-z"])
        for name in files.decode().split("\0"):
            if not name.endswith("Cargo.toml"):
                continue
            manifest = repo / name
            source = manifest.read_text()

            def bind(match):
                path = Path(os.path.abspath(manifest.parent / match[1]))
                if not path.is_relative_to(repo / "bebop"):
                    return match[0]
                target = bebop / path.relative_to(repo / "bebop")
                return f'path = "{os.path.relpath(target, manifest.parent)}"'

            bound = re.sub(r'path\s*=\s*"([^"\n]+)"', bind, source)
            if bound != source:
                manifest.write_text(bound)
    # Fail before compiling if the pinned Buckyball does not provide the schema.
    (repo / "bbdev/api/steps/config/scripts/proto/design.proto").stat()
    subprocess.run(
        [
            "cargo",
            "build",
            "--locked",
            "--release",
            "--manifest-path",
            str(bebop / "Cargo.toml"),
            "--package",
            "bebop-cli",
            "--bin",
            "bebop-bemu",
        ],
        check=True,
    )
    (bebop / "bin").mkdir(exist_ok=True)
    shutil.copy2(bebop / "target/release/bebop-bemu", bebop / "bin/bebop-bemu")
    print(f"Installed {bebop / 'bin/bebop-bemu'} using {repo}")


if __name__ == "__main__":
    main()
