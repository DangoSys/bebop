#!/usr/bin/env python3
"""Install the BEMU command for a library or standalone Bebop checkout."""

import os
from pathlib import Path
import re
import shutil
import subprocess


def bind_manifest(source, manifest, repo, bebop):
    def bind(match):
        path = Path(os.path.abspath(manifest.parent / match[1]))
        if not path.is_relative_to(repo / "bebop"):
            return match[0]
        target = bebop / path.relative_to(repo / "bebop")
        return f'path = "{os.path.relpath(target, manifest.parent)}"'

    return re.sub(r'path\s*=\s*"([^"\n]+)"', bind, source)


def main():
    bebop = Path(__file__).resolve().parent.parent
    if (bebop.parent / "bbdev").is_dir():
        repo = bebop.parent
    else:
        repo = bebop / "thirdparty/buckyball"
        if (repo / ".git").exists():
            # Undo only our exact path rebinding before updating the pinned dependency.
            # Other local edits stay intact; Git reports conflicting changes.
            changed = subprocess.check_output(
                ["git", "-C", str(repo), "diff", "--name-only", "-z"]
            )
            for name in changed.decode().split("\0"):
                if not name.endswith("Cargo.toml"):
                    continue
                manifest = repo / name
                original = subprocess.check_output(
                    ["git", "-C", str(repo), "show", f"HEAD:{name}"]
                ).decode()
                if manifest.read_text() == bind_manifest(
                    original, manifest, repo, bebop
                ):
                    manifest.write_text(original)
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
        files = subprocess.check_output(["git", "-C", str(repo), "ls-files", "-z"])
        for name in files.decode().split("\0"):
            if not name.endswith("Cargo.toml"):
                continue
            manifest = repo / name
            source = manifest.read_text()
            bound = bind_manifest(source, manifest, repo, bebop)
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
