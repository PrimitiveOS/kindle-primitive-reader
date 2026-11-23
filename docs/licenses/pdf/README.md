# PDF renderer and embedded assets

Reader uses Hayro 0.9.0 to render PDFs offline. Hayro and its companion crates
are available under the MIT or Apache 2.0 licence. Their exact source links and
licence texts are in [the dependency notices](../THIRD-PARTY.txt).

- [Standard font notice](PDFIUM-STANDARD-FONTS.txt) from
  [`hayro-font` 0.5.0](https://crates.io/crates/hayro-font/0.5.0)
- [Adobe character map notice](ADOBE-CMAPS.txt) from
  [`hayro-cmap` 0.1.1](https://crates.io/crates/hayro-cmap/0.1.1)
- [Colour profile terms](JPEG2000-COLOUR-PROFILES.txt) from
  [`hayro-jpeg2000` 0.5.0](https://crates.io/crates/hayro-jpeg2000/0.5.0)


The 32-bit ARM build reuses exact four-channel image colors before Hayro’s DeviceCMYK conversion. The interpreter’s CMYK and Lab reference profiles retain their upstream CC0 terms in the packaged dependency notices. [Profile attribution](https://github.com/LaurenzV/hayro/blob/3f3825c3e50bb41ed6ec6c3f4b6007978bd13def/hayro-interpret/assets/README.md) · [Profile license](../THIRD-PARTY.txt)

Reader uses jpeg-decoder 0.3.2 under the MIT license for oversampled, interleaved Adobe CMYK and YCCK photographs. [Packaged license](JPEG-DECODER-MIT.txt) · [Upstream source](https://github.com/image-rs/jpeg-decoder)

Reader carries the pinned Hayro PDF parser with local owner-password support. The upstream Apache 2.0 and MIT license texts are included in the packaged notices. [Parser change](../../../vendor/hayro-syntax/PRIMITIVEOS.md) · [Packaged licenses](../THIRD-PARTY.txt)
