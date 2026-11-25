# Lossless WebP allocation audit

This is an implementation proposal, not enabled support. Reader currently
accepts only a simple lossy VP8 still image through the comic/standalone image
path. KOReader registers WebP through its picture provider and its base uses
libwebp, including a frame interface. [Provider](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/picdocument.lua#L56-L62) · [KOReader WebP adapter](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/ffi/pic.lua#L276-L308)

## Allocation points

The locked `image-webp` 0.2.4 decoder parses VP8L dimensions before reading
the frame. A simple VP8L-only RIFF preflight must independently validate the
single chunk, its exact length and padding, lossless signature and version,
and width and height before calling the decoder. Reject VP8X, animation and
metadata chunks in this first scope. Proposed bounds are 4 MiB compressed,
2,048 per side, and **1,048,576 pixels**. Confirm the decoder dimensions and
RGB/RGBA layout again before allocating its output.

The decoder may keep an RGBA scratch image of `4P` bytes for an opaque RGB
result, in addition to a `3P` output; an alpha result uses a `4P` output.
The lossless transform list contains at most four distinct types. Predictor
and color-transform buffers each use four bytes per subsampled pixel; the
color table holds at most 256 RGBA entries. Color-cache sizes are limited by
the existing 1–11 bit check, at most 8 KiB per cache. Color-index expansion
also allocates a bounded 256-entry lookup and one packed image row.

The main entropy image is at most one RGBA sample per subsampled pixel, and
its two-byte Huffman group map coexists temporarily. Its decoding uses the
non-meta path, so it does not recursively create another entropy image. The
entropy values may nevertheless request up to 65,536 Huffman groups. Each
group owns five Huffman trees, so input length and dimensions alone do not
bound memory to an acceptable device budget. `WebPDecoder::set_memory_limit`
limits ICC/EXIF/XMP reads; it does not cap these lossless allocations.

A patched decoder must reject **more than 16 groups immediately after the
entropy map is decoded and before constructing the group vector**. With the
existing 11-bit cache cap, the largest alphabet sizes are 2,328, 256, 256,
256 and 40 symbols. Each tree's lookup table holds at most 1,024 `u32`s, and
its allocated branch-node capacity is at most twice its symbol count. The
maximum 16-group tables and nodes are therefore approximately 2 MiB on a
64-bit target, plus vector metadata and a temporary code-length table. The
actual Rust type sizes, allocator overhead, color-index buffers, input/output
copies, and peak resident memory must be measured on both targets before
claiming a complete memory bound. A conservative component inventory at the
proposed pixel cap is 4 MiB input + 7 MiB output/scratch + 8 MiB for both
transform buffers + 6 MiB entropy/map + 1 MiB grayscale + roughly 2 MiB
Huffman tables, plus overhead. This inventory is intentionally pessimistic;
it is not a proven process-wide memory cap or a replacement for malformed-input
and resource-limit tests.

## Other consumers

`src/online_lookup/media.rs` also accepts WebP from Wikipedia. Its limits cap
response size, dimensions and output allocation, but do not reach the VP8L
Huffman group allocation. A global crate patch would constrain this path and
the general `image` dependency as well as standalone/comic pages. Any cap must
be reviewed for these consumers and compatibility-tested against their real
images. A separate patched package used only by the comic provider avoids
changing them but duplicates codec code and leaves the online path unresolved.
An isolated, resource-limited decoder process is another option to evaluate.

## License and provenance

`image-webp` 0.2.4 is distributed under **MIT OR Apache-2.0**. The locked
registry package checksum is
`525e9ff3e1a4be2fbea1fdf0e98686a6d98b4d8f937e1bf7402245af1909e8c3`.
Its packaged `LICENSE-MIT` SHA-256 is
`c77a4cf9da729987d0fe7ccd811e3bd27393914ddf3d23467c18cc22954513b3`;
`LICENSE-APACHE` is
`0d542e0c8804e39aa7f37eb00da5a762149dc682d7829451287e11b938e94594`.
The inspected upstream `src/lossless.rs` SHA-256 is
`dd14e7d8dd72c8ec76ffc27b4847d58b9ff83818dc1b25e3f4cf6169be89ea1f`.
Any vendored copy must retain the selected upstream license and required
notices in source and release packages, identify the local patch, and update
the third-party notice. No source has been copied or modified for this audit.
