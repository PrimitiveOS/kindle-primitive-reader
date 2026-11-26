# EPUB `xml:id` targets

Links and Contents in XML-authored XHTML can target `xml:id` attributes. Reader
now indexes a unique bounded `xml:id` as an additional chapter fragment when it
does not collide with an existing unqualified ID, accepted legacy `<a name>`
or Kindle synthetic target. Duplicate, oversized, reserved or ambiguous values
are skipped; existing destination precedence, chapter bytes, block keys and
saved anchors are unchanged. Existing source-verified Contents and footnote
checks remain in force.

This is a reading compatibility candidate. KOReader behavior and real-book,
simulator and Kindle verification remain open. Its [CREngine XML parser](https://github.com/koreader/crengine/blob/b32a88ffbbf505cf2ea5208c1a49002d70d3609f/crengine/src/lvtinydom.cpp#L9465-L9490)
retains XML attribute names and namespaces; that source alone does not establish
equivalent fragment-target behavior.
