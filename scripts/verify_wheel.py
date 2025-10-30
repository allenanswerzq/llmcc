"""Verify that the built wheel contains all expected Python modules."""

from __future__ import annotations

import zipfile
from pathlib import Path


def main() -> None:
    wheel_dir = Path("target/wheels")
    wheels = sorted(wheel_dir.glob("llmcc-*.whl"))
    if not wheels:
        raise SystemExit("No wheels found in target/wheels")

    wheel = wheels[-1]
    print(f"Verifying {wheel}")

    with zipfile.ZipFile(wheel) as archive:
        names = set(archive.namelist())

    required = {
        "llmcc/__init__.py",
        "llmcc/api.py",
        "llmcc_bindings/__init__.py",
    }
    missing = required - names
    if missing:
        missing_list = ", ".join(sorted(missing))
        raise SystemExit(f"Wheel missing expected files: {missing_list}")

    print("Wheel structure OK")


if __name__ == "__main__":
    main()
