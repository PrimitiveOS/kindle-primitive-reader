#!/usr/bin/env python3
"""Build a standalone ARMHF verification bundle from Reader's Rust tests."""

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parent.parent
TESTS = ("reader", "epub_package", "layout_probe")
RUN = """#!/bin/sh
set -eu
cd "$(dirname "$0")"
test_home=$(mktemp -d /var/tmp/primitive-reader-test.XXXXXX)
trap 'rm -rf "$test_home"' EXIT
export TMPDIR="$test_home"
export READER_TEST_FIXTURES="$(pwd)/fixtures"
failed=0
for suite in reader epub_package layout_probe; do
    printf '\nReader verification: %s\n' "$suite"
    "./$suite" --test-threads=1 || failed=1
done
exit "$failed"
"""


def build(offline):
    command = [
        "cargo", "zigbuild", "--locked", "--release", "--tests",
        "--target", "armv7-unknown-linux-musleabihf", "--features", "kindle",
        "--message-format=json",
    ]
    if offline:
        command.append("--offline")
    executables = {}
    with subprocess.Popen(command, cwd=ROOT, text=True, stdout=subprocess.PIPE) as cargo:
        for line in cargo.stdout:
            try:
                item = json.loads(line)
            except json.JSONDecodeError:
                print(line, file=sys.stderr, end="")
                continue
            if item.get("reason") == "compiler-message":
                print(item["message"].get("rendered") or "", file=sys.stderr, end="")
            if item.get("reason") != "compiler-artifact" or not item.get("executable"):
                continue
            target = item["target"]
            if not item["profile"]["test"] or target["name"] not in TESTS:
                continue
            if not Path(target["src_path"]).resolve().is_relative_to(ROOT):
                continue
            executables[target["name"]] = Path(item["executable"])
        if cargo.wait() != 0:
            raise SystemExit(cargo.returncode)
    if missing := set(TESTS) - executables.keys():
        raise SystemExit("Missing Reader test suites: " + ", ".join(sorted(missing)))
    return executables


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="New output directory for the Kindle test bundle")
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists():
        parser.error("Output already exists. Select a new directory.")
    executables = build(args.offline)
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=output.parent, prefix="reader-test-stage-") as temp:
        stage = Path(temp) / "bundle"
        stage.mkdir()
        shutil.copytree(ROOT / "tests" / "fixtures", stage / "fixtures")
        for name, executable in executables.items():
            shutil.copy2(executable, stage / name)
        runner = stage / "run.sh"
        runner.write_text(RUN, encoding="utf-8")
        runner.chmod(0o755)
        stage.rename(output)
    print(output)


if __name__ == "__main__":
    main()
