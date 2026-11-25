# Reader release notices

The staged package includes the project software terms in `LICENSE` and
the project documentation and artwork terms in `LICENSE-DOCS`.

[THIRD-PARTY.txt](THIRD-PARTY.txt) records the locked Kindle dependency versions,
licences, and exact source links. The source of each MPL 2.0 component is
available from its listed crate archive. PrimitiveOS Tools is used under its
PolyForm Noncommercial option. Reader uses self_cell under its Apache 2.0
option. The USB drive-mode artwork is from PrimitiveOS
Launcher. [Launcher artwork terms](https://github.com/PrimitiveOS/kindle-primitive-launcher#licence)

- [SRELL](SRELL.txt) covers the ECMAScript engine used in reflow search. The ARMHF binary uses the same LLVM [libc++](ocr/libcxx-LICENSE), [libc++abi](ocr/libcxxabi-LICENSE) and [libunwind](ocr/libunwind-LICENSE) runtimes already distributed with OCR.
- [American and British English hyphenation patterns](ENGLISH-HYPHENATION.txt) cover the dictionaries compiled into native reflow.
- [Spanish, Italian, and Portuguese hyphenation patterns](HYPHENATION-ES-IT-PT.txt) record the upstream versions, checksums, and terms for those dictionaries.
- [Estonian and Finnish hyphenation patterns](HYPHENATION-ET-FI.txt) record the upstream versions, checksums, and terms for those dictionaries.
- [Danish and Dutch hyphenation patterns](HYPHENATION-DA-NL.txt) record the upstream versions, checksums, and terms for those dictionaries.
- [Wuff](WUFF.txt) contains the MIT terms for the isolated WOFF 2 decoder.
- [French and German hyphenation patterns](HYPHENATION-FR-DE.txt) record the upstream versions, checksums, and terms for those dictionaries.
- [Bulgarian, Galician, Irish and Ukrainian hyphenation patterns](HYPHENATION-BG-GL-GA.txt) record their individual upstream terms and checksums.
- [Norwegian hyphenation patterns](HYPHENATION-NO.txt) include the original source and its copyright and permission notice.
- [Croatian hyphenation patterns](HYPHENATION-HR.txt) include the original source under [LPPL 1.3c](LPPL-1.3c.txt).
- [Turkish hyphenation patterns](HYPHENATION-TR.txt) include the original source under [LPPL 1.3c](LPPL-1.3c.txt).
- [Catalan hyphenation patterns](HYPHENATION-CA.txt) include the original source under [LPPL 1.3c](LPPL-1.3c.txt).
- [Swedish hyphenation patterns](HYPHENATION-SV.txt) include the original source under [LPPL 1.3c](LPPL-1.3c.txt).
- [Russian hyphenation patterns](HYPHENATION-RU.txt) include the original source and [LPPL 1.3c](LPPL-1.3c.txt).
- [Polish hyphenation patterns](HYPHENATION-PL.txt) record the upstream source, checksum and selected MIT terms.
- [Slovak and Slovenian hyphenation patterns](HYPHENATION-SK-SL.txt) record their upstream sources, checksums and selected MIT terms.
- [PDF assets](pdf/README.md) covers embedded fonts, character maps, and colour
  profiles supplied by the Hayro PDF libraries.
- The KOReader statistics importer links the SQLite 3.50.2 amalgamation
  included in libsqlite3-sys 0.35.0. The SQLite sources carry the authors'
  copyright disclaimers. The Rust bindings and wrapper notices are in
  [THIRD-PARTY.txt](THIRD-PARTY.txt). [Exact crate source](https://static.crates.io/crates/libsqlite3-sys/libsqlite3-sys-0.35.0.crate)
- [Nerd Fonts Symbols](NERDFONTS.txt) covers the MIT-licensed private-use
  symbol fallback font included with Reader.
- The MIT-licensed [vendored DjVu BZZ decoder](../../vendor/djvu-bzz) pins the
  upstream source and adds Reader's decoded-output limit.
- [Rust Standard Library notices](runtime/RUST-COPYRIGHT.html),
  [Apache 2.0](runtime/RUST-APACHE.txt), and [MIT](runtime/RUST-MIT.txt) come
  from the Rust 1.100.0 distribution used for the ARMHF build.
- [musl copyright](runtime/MUSL-COPYRIGHT.txt) and [Zig MIT](runtime/ZIG-MIT.txt)
  come from the Zig 0.17.1 distribution used for the ARMHF build.

Run `python3 docs/licenses/collect.py` after changing the locked dependencies.
It uses the cached Cargo sources and fetches upstream texts for crates published
without licence files. Update the bundled asset and runtime notices when their
versions change.
