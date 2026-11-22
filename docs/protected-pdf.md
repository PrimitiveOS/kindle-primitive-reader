# Reading password-protected PDFs

Open the PDF in Library and enter its document or owner password. The keyboard masks your input. Reader keeps reading locations and local bookmarks across sessions and asks again when a new session opens a protected document. A document with an empty reading password opens directly. Reader leaves the source PDF intact. [Password prompt on the Oasis](screenshots/protected-pdf.png)

| Standard security handler | Known reader credential | Known owner credential |
| --- | --- | --- |
| Revision 2, 40-bit RC4 | Opens | Opens |
| Revision 3, 128-bit RC4 | Opens | Opens |
| Revision 4, AES-128 | Opens | Opens |
| Revision 5, AES-256 | Opens | Opens |
| Revision 6, AES-256 | Opens | Opens |

Earlier security handlers may store accented characters using PDFDocEncoding. Reader validates this representation as well as the UTF-8 representation accepted by previous builds. Revision 6 uses UTF-8. Reader's prompt accepts up to 127 characters. Modern PDF security handlers use at most 127 UTF-8 bytes. The shared Kindle keyboard determines which characters can be entered on screen. The comparison covered precomposed accented credentials. Other Unicode normalization forms and certificate-based encryption remain outside the verified set. [ISO 32000-1, section 7.6](https://udp.adobe.io/document-services/docs/assets/35e4369068f86065372c18787171a17e/PDF_ISO_32000-1.pdf) · [PDF Association security handler errata](https://pdf-issues.pdfa.org/32000-2-2020/clause07.html)

## Reference and verification

KOReader's PDF provider uses MuPDF's password test and authentication before reading document metadata. KOReader retries rejected passwords in a masked dialog. Before this change Reader rejected valid owner credentials on revisions 2, 3 and 4. Its input field also stopped at 64 characters even when the parser could open the book with a longer password. Reader now authenticates the supplied credential in the same in-memory document parser used for page rendering, search and navigation. It distinguishes unsupported encryption from a password that can be retried. Encrypted documents offer local annotations. Embedded annotation import and annotated PDF export continue to require an unencrypted source. [KOReader PDF provider](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/document/pdfdocument.lua#L35-L97) · [KOReader password dialog](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/readerui.lua#L794-L836) · [Parser change and provenance](../vendor/hayro-syntax/PRIMITIVEOS.md)

The test set contains ten independently authored three-page publications. They cover revisions 2 through 6, a 100-character reading password, an empty reading password, accented revision 4 and 6 credentials, and an unencrypted reference. Installed KOReader on the Oasis checked each case with four inputs. Twenty-three opened and seventeen were rejected as expected. On successful authentication, it opened the second page, found its text and three outline entries, and rendered the page. Reader compares all three pages, text rectangles and outlines to the unencrypted reference. Its normal reading controls also check a rejected attempt, a 100-character masked keyboard entry, Find all, jumping to page 2, Previous location, a page turn, a fresh password prompt and restoration of the saved page. Private books were excluded from these checks. [Test fixtures](../tests/fixtures/pdf-password) · [Fixture generator](../scripts/generate-protected-pdf-controls.py) · [KOReader comparison](../scripts/koreader-protected-pdf.lua)

The native Reader test runner on the Oasis passed all five protected-PDF tests. Its 1,264 × 1,680 password prompt matched the Mac simulation pixel for pixel. Source fixture bytes, the existing Reader data and the active Browser session file retained their checksums. The comparison used separate temporary reading state.

```sh
cargo test --locked --lib protected_pdf
uv run --with pypdf==7.1.1 --with cryptography==50.0.2 \
  python scripts/generate-protected-pdf-controls.py build/protected-pdf-controls
```

Regeneration uses fresh encryption salts and file identifiers. The page contents and known test credentials remain the same.
