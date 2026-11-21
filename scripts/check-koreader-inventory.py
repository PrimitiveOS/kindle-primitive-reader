#!/usr/bin/env python3
"""Compare the table of upstream identifiers against a pinned KOReader Git tree."""
import argparse
from pathlib import Path
import re
import subprocess

REV = "9192014d8bd82a91dc1012473be0f238dedfdb54"
TABLE = Path(__file__).resolve().parents[1] / "docs" / "koreader-source-inventory.md"


def inventory(repo):
    def git(*args):
        return subprocess.check_output(["git", "-C", str(repo), *args], text=True)

    def source(path):
        return git("show", f"{REV}:{path}")

    def directory(path, suffix):
        files = git("ls-tree", "--name-only", f"{REV}:{path}").splitlines()
        return {f[:-len(suffix)] for f in files if f.endswith(suffix)}

    groups = {
        "Reader modules": directory("frontend/apps/reader/modules", ".lua"),
        "File manager modules": directory("frontend/apps/filemanager", ".lua"),
        "Document modules": directory("frontend/document", ".lua"),
        "Bundled plugins": directory("plugins", ".koplugin"),
    }
    for name, path in [("Reflow controls", "creoptions"), ("Fixed-page controls", "koptoptions")]:
        content = source(f"frontend/ui/data/{path}.lua")
        groups[name] = set(re.findall(r'^ {16}name = "([\w]+)",', content, re.M))
    menu = source("frontend/ui/elements/reader_menu_order.lua")
    for group, block in re.findall(r'^    (\w+) = \{([^}]+)\}', menu, re.M):
        groups["Menu " + group] = set(re.findall(r'^        "(\w+)"', block, re.M))
    return groups


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("repo", type=Path, help="KOReader checkout containing the reference commit")
    args = parser.parse_args()
    expected = inventory(args.repo)
    current = None
    actual = {}
    for line in TABLE.read_text().splitlines():
        if line.startswith("## "):
            current = line[3:]
            actual[current] = []
        elif current and line.startswith("| `"):
            first_cell = line.split("|")[1]
            actual[current].extend(re.findall(r"`(\w+)`", first_cell))
    failures = []
    for group, names in expected.items():
        listed = actual.get(group, [])
        if len(set(listed)) != len(listed):
            failures.append(f"{group} has duplicate entries")
        if missing := names - set(listed):
            failures.append(f"{group} missing {sorted(missing)}")
        if extra := set(listed) - names:
            failures.append(f"{group} unexpected {sorted(extra)}")
    for extra in set(actual) - set(expected):
        failures.append(f"Unexpected table {extra}")
    if failures:
        raise SystemExit("\n".join(failures))
    for group, names in expected.items():
        print(f"{group}: {len(names)} identifiers")
    print(f"Matched {sum(map(len, expected.values()))} entries at {REV}")


if __name__ == "__main__":
    main()
