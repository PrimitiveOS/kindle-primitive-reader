# Install Reader

Reader 0.1.2 targets the Kindle Oasis 3 on firmware 5.18.2 with an existing
PrimitiveOS Launcher. It adds a Documents browser with folder icons, file
types, file sizes, and labels for files Reader cannot open. Reading controls
are grouped in compact bars and contextual panels over the current page.

Use Reader in PrimitiveOS Store, or follow the manual install steps below.
[Reader 0.1.2 release](https://github.com/PrimitiveOS/kindle-primitive-reader/releases/tag/v0.1.2)

1. Obtain `primitive-reader-0.1.2-armhf.zip` and its `.sha256` file from the
   same build. Verify the archive from the directory containing both files
   with `shasum -a 256 -c primitive-reader-0.1.2-armhf.zip.sha256` on macOS or
   `sha256sum -c primitive-reader-0.1.2-armhf.zip.sha256` on Linux.
2. Extract the ZIP and open the `primitive-reader-0.1.2/` folder. Connect your
   Kindle over USB. Copy the `reader/` folder to the root of the Kindle drive and copy
   `documents/Reader.sh` into the existing `documents/` folder. When upgrading,
   copy the new package files into the existing `reader/` folder and keep
   `reader/data/`, where Reader saves progress, annotations, and dictionaries.
3. Put a supported book in the Kindle's `documents/` folder. Eject the drive
   safely, return to Launcher, and open **Reader**.

The installed third-party notices are in `reader/licenses/`. The archive also
contains the project software and documentation licences.

## Build and package

Install the Rust `armv7-unknown-linux-musleabihf` target, Zig, and `cargo-zigbuild`.
Clone `doannt-kindle-build` alongside this repository and install `jsonschema`
in the Python environment used for packaging. Run these commands from the Reader
repository root.

```sh
cargo zigbuild --locked --release --target armv7-unknown-linux-musleabihf \
  --features kindle --bin reader-kindle --bin reader-woff2-worker
sh scripts/stage.sh armhf
python3 ../doannt-kindle-build/scripts/package.py \
  --id primitive-reader --dir reader --version 0.1.2 --arch armhf \
  --firmware-min 5.18.2 --stage build/stage --license LICENSE \
  --docs-license LICENSE-DOCS --install INSTALL.md --out build/dist
```

Cargo and the staging script both use `CARGO_TARGET_DIR` when set. The last
command writes the ARMHF archive and its checksum into `build/dist/`.

Use the [OCR build instructions](docs/ocr-build.md) to include the
English recognition engine and model in the stage passed to the packager.
