# Dictionary downloads

Reader has a native download flow for KOReader's pinned StarDict catalogue
(`9192014d8bd82a91dc1012473be0f238dedfdb54`). On the first visit, it fetches
the catalogue and ISO language names in parallel, then parses them away from
the Reader event loop. The resulting 803 entries are cached in Reader's local
dictionary data directory and can be refreshed from the catalogue screen.
The parser requires exactly 803 records and verifies SHA-256 digests for both
raw source files against the pinned revision before accepting or caching the
catalogue. A cache from a different source snapshot is ignored.
The 799 HTTPS entries can be installed; the four HTTP-only entries remain
visible with installation disabled. Blank license fields remain visible as
"Not stated in the KOReader catalogue" rather than being silently omitted.
The source Lua and catalogue data are not bundled in the Reader package.

The Dictionaries screen groups available packages by source or target
language, as KOReader does. Each language list can show **All**,
**Same-language**, or **Bilingual** dictionaries. Rows show the entry count,
language pair, and installed state. Each package detail shows its language
pair, entry count, the source catalogue's license field, package URL, and
pinned catalogue source before the user chooses **Check package size**. Reader
checks the archive over HTTPS in a bounded background request, then shows the
reported size before enabling **Download and install**. If the server does not
provide a size or rejects the check, the user can retry or explicitly proceed
without an estimate. The download still enforces the compressed archive limit.
Only the small catalogue metadata cache is retained on the device; dictionary
archives are downloaded only after the explicit install action.

Downloads use HTTPS with a 90-second request timeout and a 32 MiB compressed
archive limit. The worker streams the response to a uniquely named staging
file in the Reader's dictionary data directory, so it does not hold the full
compressed package in memory. The installer dispatches `.tar.gz` / `.tgz`, `.tar.bz2` /
`.tbz2`, `.tar.zst` / `.tzst`, and `.zip` packages. TAR archives must use
USTAR headers; the installer rejects invalid checksums, absolute or
parent-traversal paths, duplicate files, links and unsupported TAR entry types.
Zstandard decompression is capped at a 64 MiB window. ZIP extraction accepts
stored, Deflate, Bzip2 and Zstandard members, and rejects links and special
files. It limits an archive to 512 entries, 64 MiB per file and 128 MiB
expanded. It validates a StarDict `.ifo`, `.idx`, and `.dict` or `.dict.dz`
set before an atomic rename into `reader/data/dictionaries/<package-id>/`.
Failed installs remove only their own staging directory. A generated
`ATTRIBUTION.txt` records the package title, catalogue license field, package URL,
and catalog revision; files supplied by the archive, including any separate
license text, remain alongside the dictionary.

During transfer, the package detail displays the number of bytes received and,
when the server provides `Content-Length`, the total size. The worker reads the
response in bounded 16 KiB chunks and emits throttled progress updates through
its stderr pipe. The Reader event loop only consumes those updates and requests
a partial redraw when the displayed value changes.

KOReader's pinned download flow first sends a `HEAD` request, requires a
`Content-Length`, and asks the user to confirm the reported file size before
starting the `GET`. Reader now follows that review step while allowing an
explicit proceed-without-estimate path for servers that omit or reject `HEAD`.
Its size check has a 20-second request timeout; the archive `GET` remains
bounded to 90 seconds and 32 MiB. [Pinned KOReader confirmation flow](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerdictionary.lua#L1678-L1758)

The detail screen offers a cooperative **Cancel download** action. Leaving
Reader also requests cancellation. The worker checks cancellation before and
after the HTTPS request and between bounded archive-copy chunks, removes its
staging directory, and releases the storage lease on its normal return path.
An in-flight HTTPS request is bounded by the 90-second request timeout, so a
cancel request can take until that request returns before cleanup completes.

The catalog's license and public-domain descriptions are presented as source
metadata and are not a legal determination by Reader. Downloaded data has its
own license, separate from the PrimitiveOS Reader source license. The user
chooses each download explicitly and should check the source's terms before
redistributing the package.

## Parity and remaining work

The KOReader menu at `T5-1.1.6` now has an implementation candidate for all
803 pinned catalogue records, source/target language grouping, monolingual and
bilingual filters, entry counts, license details, install-state indicators,
and pre-download size confirmation. Four HTTP-only sources are shown but
cannot be installed. The Lua parser, pinned-source digest checks, asynchronous
catalogue refresh and size preflight, cache behavior, archive paths, package
size limits, cancellation and end-to-end behavior remain unverified. This
candidate has not been built, tested, simulated, or installed on a Kindle.

KOReader references:

- [Dictionary settings implementation at the pinned revision](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerdictionary.lua#L275-L309)
- [Pinned dictionary package catalog](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/dictionaries.lua)
