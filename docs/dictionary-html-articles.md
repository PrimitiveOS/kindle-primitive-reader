# HTML dictionary articles

StarDict HTML definitions show numbered senses, bullets, and indented examples
and translations. An example nested inside a sense uses a deeper indent. Decimal
lists can set a starting number or override an individual item's number. Reader
wraps long entries within the existing dictionary popup.

Reader retains a block's language and writing direction. Explicit direction
applies to every wrapped line. With automatic direction, Reader checks the
paragraph's text. Declared language also supplies a direction for otherwise
neutral text when no direction is set. The shared Primitive UI renderer supplies
the definition font and its available script fallbacks.

Find and long-press lookup use the same definition text as before. Bullets and
numbers sit in the margin so they do not enter search results. Going back from a
nested lookup restores the result, its position, and any active Find highlight.
Reader extracts the supported formatting locally from the article text and drops
the formatting metadata if its bounds are exceeded. [KOReader HTML definition handling](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerdictionary.lua#L1041-L1084) · [KOReader list and example styling](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/dictquicklookup.lua#L1142-L1208)

The desktop test opens an authored bilingual dictionary from an EPUB selection.
It verifies two numbered senses, an indented Arabic translation that wraps,
Find, a long press within that translation, and pixel-exact return to the first
lookup. Parser tests include malformed nesting, oversized attribute values, and
inert script, style, and resource references. [Kindle-sized desktop capture](screenshots/dictionary-structured-article.png) · [Popup tests](../src/app/dictionary_panel/tests.rs)

## Cross-references

Tap an underlined `bword://` reference or a plain, unqualified headword link to
look up its target in the dictionary that supplied the current definition.
Reader accepts case variants and indexed
synonyms through its exact lookup. Back restores the previous definition, its
scroll position and Find highlight. A missing target leaves the current
definition open. External links containing `://` remain plain text. Unqualified
headwords are treated literally, including percent characters. The added
unqualified-link behavior awaits integrated testing.
[KOReader reference handler](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerdictionary.lua#L863-L898)

An authored desktop workflow taps a reference that wraps across two lines,
distinguishes two dictionaries with the same display name, and verifies that
Back restores both the definition and the selected passage. [Cross-reference popup](screenshots/dictionary-cross-reference.png) · [Workflow test](../src/app/dictionary_panel/tests.rs)
