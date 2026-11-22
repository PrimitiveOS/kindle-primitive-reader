#!/usr/bin/env python3
"""Exercise Reader against an isolated, loopback-only official calibre Content server.

No installation/download occurs here. Supply the directory containing calibre-server
and calibredb, freshly built Reader test/worker executables and an authorized EPUB.
Only temporary copies enter the isolated library. The temporary library/config/user
DB and acquired books are removed; receipts, test logs and sanitized UI captures remain.
"""
import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import shutil


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("calibre-bin", "tests", "worker", "original", "output"):
        parser.add_argument("--" + name, type=Path, required=True)
    args = parser.parse_args()
    for name in ("calibre_bin", "tests", "worker", "original", "output"):
        setattr(args, name, getattr(args, name).resolve())
    args.output.mkdir(parents=True, exist_ok=True)
    original_sha = sha(args.original)
    work = Path(tempfile.mkdtemp(prefix="private-calibre-", dir=args.output))
    env = dict(os.environ, CALIBRE_CONFIG_DIRECTORY=str(work / "config"),
               CALIBRE_TEMP_DIR=str(work / "temp"), QT_QPA_PLATFORM="offscreen")
    (work / "temp").mkdir()
    server = args.calibre_bin / "calibre-server"
    db = args.calibre_bin / "calibredb"
    process = None
    receipt = {"original_sha256": original_sha, "original_bytes": args.original.stat().st_size,
               "tests_sha256": sha(args.tests), "worker_sha256": sha(args.worker), "cases": []}

    def command(argv, **kwargs):
        result = subprocess.run([str(a) for a in argv], env=env, capture_output=True,
                                text=True, timeout=120, **kwargs)
        if result.returncode:
            raise RuntimeError(result.stderr[-2000:])
        return result.stdout

    def stop():
        nonlocal process
        if process:
            process.terminate()
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
            process = None

    try:
        receipt["provider_version"] = command([server, "--version"]).strip()
        for ident, title in enumerate(("Original EPUB verification", "Second catalog publication"), 1):
            command([db, "add", "--with-library", work / "library", "--duplicates",
                     "--title", title, "--authors", "PrimitiveOS verification", args.original])
            command([db, "set_metadata", "--with-library", work / "library", ident,
                     "--field", "comments:", "--field", "tags:"])
        command([server, "--userdb", work / "users.sqlite", "--manage-users", "--", "add",
                 "reader", "--readonly"], input="catalog secret")
        for authenticated in (False, True):
            with socket.socket() as sock:
                sock.bind(("127.0.0.1", 0))
                port = sock.getsockname()[1]
            case = "basic" if authenticated else "anonymous"
            url = f"http://127.0.0.1:{port}/opds"
            argv = [str(server), "--listen-on", "127.0.0.1", "--port", str(port),
                    "--disable-fallback-to-detected-interface", "--disable-use-bonjour",
                    "--max-opds-items", "1", "--access-log", str(work / "access.log")]
            if authenticated:
                argv += ["--enable-auth", "--auth-mode", "basic", "--userdb", str(work / "users.sqlite")]
            else:
                argv += ["--disable-auth"]
            argv += [str(work / "library")]
            with (args.output / f"{case}-server.log").open("wb") as log:
                process = subprocess.Popen(argv, env=env, stdout=log, stderr=subprocess.STDOUT)
                for _ in range(100):
                    try:
                        urllib.request.urlopen(url, timeout=1).close()
                        break
                    except urllib.error.HTTPError as error:
                        if authenticated and error.code == 401:
                            break
                        raise
                    except urllib.error.URLError:
                        if process.poll() is not None:
                            raise RuntimeError("calibre-server stopped before startup")
                        time.sleep(.1)
                else:
                    raise RuntimeError("calibre-server startup timed out")
                acquired = work / f"{case}-download.epub"
                test_env = dict(env, READER_OPDS_WORKER=str(args.worker),
                                READER_OPDS_ORIGINAL=str(args.original),
                                READER_OPDS_CALIBRE_URL=url,
                                READER_OPDS_CALIBRE_DOWNLOAD=str(acquired),
                                READER_OPDS_CALIBRE_SCREENSHOT=str(args.output / f"{case}-catalog.png"))
                test_env.pop("READER_OPDS_CALIBRE_AUTH", None)
                if authenticated:
                    test_env["READER_OPDS_CALIBRE_AUTH"] = "1"
                test = subprocess.run([str(args.tests), "opds_actual_calibre_catalog_search_pagination_acquisition_and_resume",
                                       "--ignored", "--test-threads=1"], env=test_env,
                                      capture_output=True, text=True, timeout=120)
                (args.output / f"{case}-ui.log").write_text(test.stdout + test.stderr)
                if test.returncode:
                    raise RuntimeError(f"Reader {case} provider workflow failed; see UI log")
                request = urllib.request.Request(f"http://127.0.0.1:{port}/get/epub/1/library")
                if authenticated:
                    request.add_header("Authorization", "Basic " + base64.b64encode(b"reader:catalog secret").decode())
                with urllib.request.urlopen(request, timeout=20) as response:
                    served = response.read(64 * 1024 * 1024 + 1)
                served_sha = hashlib.sha256(served).hexdigest()
                assert served_sha == sha(acquired), "Acquisition differs from the provider's response"
                assert sha(args.original) == original_sha, "The source original changed"
                receipt["cases"].append({"auth": case, "binding": "127.0.0.1", "publication_bytes": len(served),
                                         "served_sha256": served_sha, "download_sha256": sha(acquired),
                                         "pagination_search_details_download_spine_and_resume": "passed",
                                         "invalid_credentials": "passed" if authenticated else "not applicable"})
                stop()
        receipt["source_unchanged"] = sha(args.original) == original_sha
        receipt["servers_stopped"] = True
    finally:
        stop()
        shutil.rmtree(work)
        receipt["temporary_library_config_and_downloads_removed"] = not work.exists()
        (args.output / "calibre-provider.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt, indent=2))


if __name__ == "__main__":
    main()
