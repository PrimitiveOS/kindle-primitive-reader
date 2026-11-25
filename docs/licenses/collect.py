#!/usr/bin/env python3
"""Write Reader's ARMHF dependency notices from the locked Cargo graph."""

from collections import defaultdict
import json
from pathlib import Path
import subprocess
from urllib.request import urlopen


HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
RAW = "https://raw.githubusercontent.com/"
UPSTREAM = {
    "alloc-stdlib": "dropbox/rust-alloc-no-stdlib/ae42d22078b98549e987d2f03d12df7b984fde47/LICENSE",
    "anyrender": "dioxuslabs/anyrender/cf0f14776102ec775af61bcdcaac2ab345f4fafb/LICENSE-MIT",
    "anyrender_vello_cpu": "dioxuslabs/anyrender/cf0f14776102ec775af61bcdcaac2ab345f4fafb/LICENSE-MIT",
    "debug_timer": "dioxuslabs/blitz/078b082ced8cc1e35b87c4d5fd85b9cffafc9e47/LICENSE-MIT",
    "taffy": "DioxusLabs/taffy/0064bd54cbe38bf1d8bcd6ec6ab0704d45fb048a/LICENSE",
    "void": "reem/rust-void/a6e061227f47ba8798b7e828ed0ac4e25382eb15/LICENSE-MIT",
}
BLITZ_MIT = "dioxuslabs/blitz/92b11f1bd8ab7975411328b0406a95a09445b4e7/LICENSE-MIT"
MPL = "https://www.mozilla.org/media/MPL/2.0/index.815ca599c9df.txt"
PREFIXES = ("LICENSE", "COPYING", "NOTICE", "COPYRIGHT", "UNLICENSE", "PATENTS")


def dependency_graph():
    metadata = json.loads(subprocess.check_output([
        "cargo", "metadata", "--locked", "--offline", "--format-version", "1",
        "--features", "kindle", "--filter-platform", "armv7-unknown-linux-musleabihf",
    ], cwd=ROOT))
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    root = metadata["resolve"]["root"]
    visited = {root}
    pending = [root]
    while pending:
        for dep in nodes[pending.pop()]["deps"]:
            package_id = dep["pkg"]
            if package_id in visited or all(kind["kind"] == "dev" for kind in dep["dep_kinds"]):
                continue
            visited.add(package_id)
            pending.append(package_id)
    return sorted(
        (package for package in metadata["packages"] if package["id"] in visited - {root}),
        key=lambda package: (package["name"], package["version"], package["source"] or ""),
    )


def read_notices(package, fetched):
    name, source = package["name"], package["source"] or ""
    crate = Path(package["manifest_path"]).parent
    if source.startswith("git+https://github.com/PrimitiveOS/kindle-primtitive-tools.git"):
        path = next(parent / "LICENSE-POLYFORM" for parent in crate.parents
                    if (parent / "LICENSE-POLYFORM").is_file())
        return [(path.name, path.read_text(encoding="utf-8"))]

    if name == "qrcodegen":
        text = (crate / "src/lib.rs").read_text(encoding="utf-8")
        header = text.split("*/", 1)[0].removeprefix("/*")
        notice = "\n".join(line.strip().removeprefix("*").lstrip() for line in header.splitlines())
        if "Copyright (c) Project Nayuki" not in notice or "Permission is hereby granted" not in notice:
            raise ValueError("qrcodegen source notice is missing")
        return [("src/lib.rs", notice.strip())]

    paths = sorted(path for path in crate.rglob("*")
                   if path.is_file() and path.name.upper().startswith(PREFIXES))
    if name == "hayro-interpret":
        paths.append(crate / "assets" / "CGATS_LICENSE.txt")
    if package["license_file"]:
        path = crate / package["license_file"]
        if path not in paths:
            paths.append(path)
    if any(path.parent == crate for path in paths):
        return [(str(path.relative_to(crate)), path.read_text(encoding="utf-8")) for path in paths]

    if package["license"] == "MPL-2.0":
        url = MPL
    elif name.startswith("blitz-") or name == "stylo_taffy":
        url = RAW + BLITZ_MIT
    elif name.startswith("djvu-") and package["version"] == "0.46.0":
        url = RAW + "matyushkin/djvu-rs/c8e86c93214f9f10093c2c696945cfef0c9e500d/LICENSE"
    else:
        url = RAW + UPSTREAM[name]
    if url not in fetched:
        with urlopen(url, timeout=15) as response:
            fetched[url] = response.read().decode("utf-8")
    return [(url, fetched[url])] + [
        (str(path.relative_to(crate)), path.read_text(encoding="utf-8")) for path in paths
    ]


def collect():
    sections = [
        "Reader ARMHF third-party dependency notices",
        "",
        "The index covers the locked Kindle target graph, including build dependencies.",
        "The listed licences apply to their own components. PrimitiveOS Tools is used",
        "under its PolyForm Noncommercial 1.0.0 option. The individual source archives",
        "below provide the source of each MPL-2.0 component used in this package.",
        "For the compiled US and British English patterns and TUGboat exceptions, see ENGLISH-HYPHENATION.txt.",
        "For the compiled Spanish, Italian, and Portuguese patterns, see HYPHENATION-ES-IT-PT.txt.",
        "",
    ]
    fetched = {}
    notices = defaultdict(list)
    for package in dependency_graph():
        name, version, source = package["name"], package["version"], package["source"] or ""
        if not source and name == "djvu-bzz":
            source = "https://github.com/matyushkin/djvu-rs/tree/c8e86c93214f9f10093c2c696945cfef0c9e500d/crates/djvu-bzz"
        if not source and name == "libchm":
            source = "https://github.com/trypsynth/libchm/tree/54164731f2defd92856a63eab8e7cee9a2df2896"
        if not source and name == "hayro-interpret":
            source = "https://github.com/LaurenzV/hayro/tree/3f3825c3e50bb41ed6ec6c3f4b6007978bd13def/hayro-interpret"
        if not source and name == "hayro-syntax":
            source = "https://github.com/LaurenzV/hayro/tree/3f3825c3e50bb41ed6ec6c3f4b6007978bd13def/hayro-syntax"
        if not source and name == "parley":
            source = "https://github.com/linebender/parley/tree/6e1143dda817360dd3a2a64f9133c6e5dfabd881/parley"
        if source.startswith("registry+"):
            source = f"https://static.crates.io/crates/{name}/{name}-{version}.crate"
        elif source.startswith("git+"):
            repository, commit = source[4:].split("#", 1)
            source = repository.split("?", 1)[0].removesuffix(".git") + "/tree/" + commit
        sections += [f"{name} {version} ({package['license']})", source]
        for label, notice in read_notices(package, fetched):
            notices[notice.strip()].append(f"{name} {version} ({label})")
        sections.append("")
    for notice, components in notices.items():
        sections += ["-" * 72, *components, "", notice, ""]
    text = "\n".join(sections)
    (HERE / "THIRD-PARTY.txt").write_text(
        "\n".join(line.rstrip() for line in text.split("\n")), encoding="utf-8"
    )


if __name__ == "__main__":
    collect()
