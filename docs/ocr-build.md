# Reader's offline OCR build

Reader uses a separate static Tesseract process for recognizing text on a PDF
or DjVu page. The app sends a bounded grayscale raster, imports the word boxes,
and keeps recognized text with the document's reading data. Recognition can
be cancelled. The text can be searched, selected, or opened in the page's
reflowed text view. Reading data offers a way to clear the OCR cache.

| Input | Version and source |
| --- | --- |
| Tesseract | 5.5.4 at [the pinned commit](https://github.com/tesseract-ocr/tesseract/tree/7e548e6eff172f38f97664afe7f7ce40db36c8d6) |
| Leptonica | 1.87.0 at [the pinned commit](https://github.com/DanBloomberg/leptonica/tree/13275a278eb55b5746e33f95fbf5a2c8f604b3ab) |
| English model | tessdata_fast 4.1.0 at [the pinned commit](https://github.com/tesseract-ocr/tessdata_fast/tree/65727574dfcd264acbb0c3e07860e4e9e9b22185) |
| Cross toolchain | Zig 0.17.1, Cortex-A7, Linux 4.1.15 and ARM hard-float musl |

The build script checks the source and model hashes. It requires Zig, CMake,
Ninja, curl and Python 3.9 or newer.

```sh
sh scripts/build-ocr-armhf.sh build/ocr-armhf
READER_OCR_BUILD_DIR=build/ocr-armhf sh scripts/stage.sh armhf build/stage-with-ocr
```

The OCR stage contains the static executable, English model and separate
[upstream notices](licenses/ocr). A package can omit the OCR assets. Its PDF and
DjVu readers still render pages and read text layers already present in files.

Install additional compatible Tesseract `.traineddata` files in
`reader/ocr/tessdata/` alongside the English model. Names use lowercase
language codes such as `vie.traineddata` or `chi_sim.traineddata`. Each model
must be a regular file of up to 16 MiB. Open OCR from the reading bar to pick
among the installed languages. Reader stages only the selected model and
keeps its language code with the recognized page text. Additional models are
provided separately under their original terms.

The desktop OCR worker recognized 316 words from a public-domain scan with the
pinned English model. On macOS with Tesseract 5.5.2 it also recognized all 28
words across four Vietnamese test lines using the pinned `vie.traineddata`
from the same tessdata_fast revision. The ARMHF engine and package passed
build and archive checks. Kindle runtime and memory measurements remain in the
consolidated device verification. [Vietnamese model source](https://github.com/tesseract-ocr/tessdata_fast/blob/65727574dfcd264acbb0c3e07860e4e9e9b22185/vie.traineddata)
