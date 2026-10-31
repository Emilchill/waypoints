#!/usr/bin/env python3
"""Build the starter mod as an importable Cinnabar WebAssembly component."""

from pathlib import Path
import os
import shlex
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def main() -> None:
    wrapper = shlex.split(os.environ.get("CINNABAR_CARGO_WRAPPER", ""))
    cargo = [*wrapper, "cargo"]
    target = ROOT / "target"
    environment = dict(os.environ, CARGO_TARGET_DIR=str(target))
    subprocess.run(
        [
            *cargo,
            "build",
            "--locked",
            "--release",
            "-p",
            "waypoints",
            "--target",
            "wasm32-unknown-unknown",
        ],
        cwd=ROOT,
        env=environment,
        check=True,
    )
    core = target / "wasm32-unknown-unknown/release/waypoints.wasm"
    output = ROOT / "dist/waypoints.wasm"
    subprocess.run(
        [
            *cargo,
            "run",
            "--locked",
            "--release",
            "-p",
            "waypoints-pack",
            "--",
            str(core),
            str(output),
        ],
        cwd=ROOT,
        env=environment,
        check=True,
    )
    print(f"Built {output}")


if __name__ == "__main__":
    main()
