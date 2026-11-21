# SRELL

Reader uses Nozomu Katoo's SRELL 3.018 to support the ECMAScript syntax of KOReader's reflow document search. The header is byte-for-byte the copy in koreader-base at 6e4bc81af4e04f78d81677a323a780a71b29702a. Only the main header is needed when compiling with SRELL_NO_UNICODE_DATA, the setting used by CREngine. Reader maintains its own input checks, text projection and Rust interface. [Pinned header](https://github.com/koreader/koreader-base/blob/6e4bc81af4e04f78d81677a323a780a71b29702a/thirdparty/srell/srell.hpp) · [License and copyright notice](../../docs/licenses/SRELL.txt)
