#!/usr/bin/env python3
"""Run the pinned native KOReader progress widget in a fresh SDL emulator profile."""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path, help="pinned koreader-macos-reference directory")
    parser.add_argument("frame", type=Path, help="destination for the stock framebuffer PNG")
    options = parser.parse_args()
    reference = options.reference.resolve()
    runtime = reference / "runtime/koreader"
    fingerprints = {
        "main.lua": "da6db5e381bcfdaf05053d23bf75781166cc713b14076a65fcd2c527067fd088",
        "readerprogress.lua": "2edf8d29ef8374d921cb14cfb2f928c42c05b7259683b4566be05a7dbf9643d9",
    }
    for name, sha in fingerprints.items():
        path = runtime / "plugins/statistics.koplugin" / name
        if hashlib.sha256(path.read_bytes()).hexdigest() != sha:
            parser.error(f"Expected the pinned v2026.07.1 runtime file {path.name}")
    script = Path(__file__).resolve().with_suffix(".lua")
    fixture = script.parent.parent / "tests/fixtures/field-notes-epub3.epub"
    with tempfile.TemporaryDirectory(prefix="primos-stock-progress-") as directory:
        root = Path(directory)
        home = root / "profile"
        patches = home / "patches"
        patches.mkdir(parents=True)
        shutil.copyfile(script, patches / "2-qa-reading-progress.lua")
        book = root / "Field Notes.epub"
        shutil.copyfile(fixture, book)
        (home / "settings.reader.lua").write_text(
            'return { language="en", emulator_fake_wifi_connected=false, color_rendering=false }\n'
        )
        environment = dict(
            os.environ,
            KO_HOME=str(home),
            KO_DYLD_PREFIX=str(reference / "build/staging"),
            EMULATE_READER_W="1264",
            EMULATE_READER_H="1680",
            EMULATE_READER_DPI="300",
            EMULATE_BB_TYPE="BB8",
            EMULATE_BW_SCREEN="1",
            QA_READING_PROGRESS_FRAME=str(options.frame.resolve()),
            QA_READING_PROGRESS_SETTINGS=str(home / "settings"),
            SDL_VIDEODRIVER="dummy",
            TZ="UTC",
        )
        subprocess.run([runtime / "luajit", "reader.lua", book], cwd=runtime, env=environment, check=True, timeout=30)


if __name__ == "__main__":
    main()
