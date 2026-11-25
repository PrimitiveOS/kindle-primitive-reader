# KOReader element-boundary positions

KOReader CREngine's current normalized XPointer format permits an element
point `.N`: N is a boundary among that element's children. Reader can now
import nonzero boundaries in a conservative subset of unstyled, simple XHTML
inline paragraphs. This can recover bookmark and annotation endpoints at a
publisher's formatting boundary instead of rejecting every such location.

The implementation refuses styled chapters, hidden content, foreign nodes,
comments, CDATA, whitespace-only text nodes, unusual inline elements, long
split text nodes, and out-of-range positions. It uses Reader's normal source
anchor projection, which must preserve the original text and block identity.
These restrictions prevent a guessed child index from silently selecting a
different passage when CREngine repairs or wraps the source. Other structural
pointers remain unsupported.

This is an unverified implementation candidate. Focused tests cover inline
boundaries, entities, Unicode, and refusal cases; native KOReader comparison,
simulator execution, and Kindle acceptance are pending. [CREngine point
resolution](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvtinydom.cpp#L11356-L11439) · [Reader implementation and tests](../src/cre_pointer.rs)
