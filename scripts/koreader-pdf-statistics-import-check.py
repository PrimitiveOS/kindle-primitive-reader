#!/usr/bin/env python3
"""Generate the checkpointed PDF journal with the pinned native stock runtime."""
import argparse
import hashlib
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import tempfile


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("reference", type=Path, help="pinned koreader-macos-reference folder")
    parser.add_argument("output", type=Path, help="path for the resulting SQLite fixture")
    arguments = parser.parse_args()
    reference = arguments.reference.resolve()
    source_root = Path(__file__).resolve().parent.parent
    revision = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=reference / "sources/koreader", text=True
    ).strip()
    if revision != "9192014d8bd82a91dc1012473be0f238dedfdb54":
        raise SystemExit(f"Unexpected KOReader revision {revision}")

    with tempfile.TemporaryDirectory(prefix="reader-stock-pdf-stats-") as work_path:
        work = Path(work_path)
        home = work / "home"
        (home / "patches").mkdir(parents=True)
        shutil.copyfile(
            source_root / "scripts/koreader-pdf-statistics-import-check.lua",
            home / "patches/2-pdf-statistics-fixture.lua",
        )
        pdf = work / "Survey.pdf"
        shutil.copyfile(source_root / "tests/fixtures/scan-reflow/vector.pdf", pdf)
        (home / "settings").mkdir()
        runtime = reference / "runtime/koreader"
        env = dict(
            os.environ,
            KO_HOME=str(home),
            KO_DYLD_PREFIX=str(reference / "build/staging"),
            EMULATE_READER_W="632",
            EMULATE_READER_H="840",
            EMULATE_READER_DPI="300",
            EMULATE_BW_SCREEN="1",
            EMULATE_BB_TYPE="BB8",
            QA_PDF_SQLITE_SETTINGS=str(home / "settings"),
            TZ="UTC",
        )
        subprocess.run(
            [str(runtime / "luajit"), "reader.lua", str(pdf)],
            cwd=runtime,
            env=env,
            check=True,
            timeout=35,
        )
        source = home / "settings/statistics.sqlite3"
        for suffix in ("-wal", "-journal"):
            sidecar = source.with_name(source.name + suffix)
            assert not sidecar.exists() or sidecar.stat().st_size == 0
        with sqlite3.connect(f"{source.as_uri()}?mode=ro&immutable=1", uri=True) as db:
            raw = db.execute(
                "SELECT page, start_time, duration, total_pages FROM page_stat_data ORDER BY start_time"
            ).fetchall()
            assert raw == [(1, 1769990394, 12, 2), (2, 1769990406, 18, 2), (1, 1769990424, 30, 2)]
            assert db.execute("SELECT sum(duration), count(DISTINCT page) FROM page_stat").fetchone() == (60, 2)
        shutil.copyfile(source, arguments.output)
        print(f"Stock {revision}: {raw}, SHA-256 {hashlib.sha256(source.read_bytes()).hexdigest()}")


if __name__ == "__main__":
    main()
