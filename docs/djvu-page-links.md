# DjVu page links

Reader recognizes rectangular DjVu map-area links in page annotations. Local
`#` destinations use the same page ID, title, name, physical page and relative
page rules as the contents list. They participate in the fixed-page tap,
link-list and nearest-link controls, including the existing navigation history.
HTTP and HTTPS addresses use the existing external-link address screen; merely
opening or tapping the page does not make a network request.

DjVu annotations use bottom-left coordinates. Reader converts a rectangle to
top-left coordinates, applies the page's recorded orientation, and uses the
normal fixed-page transform for zoom, crop and screen rotation. Invalid,
out-of-page and nonrectangular areas are skipped. A page's decoded links are
cached only while it remains the last requested page. The importer caps the
annotation payload at 256 KiB and the page's map areas at 1,024; the existing
bounded BZZ decoder applies before compressed annotations are parsed.

KOReader's pinned DjVu provider uses its DjVuLibre binding. The inspected
provider and binding expose page rendering and text, but do not establish an
equivalent map-area link interface; this feature is not claimed as verified
KOReader parity. [KOReader provider](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/djvudocument.lua#L10-L43) · [Pinned DjVu binding](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/djvu.c)

The change has not been built or tested. Rotated-page coordinates, relative
destinations, link history and malformed annotation limits need simulator and
device verification before release.
