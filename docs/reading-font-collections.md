# Reading with a TrueType Collection

Copy a `.ttc` file to `reader/data/fonts`, open the reading font chooser and
select its row. Reader previews its first face, registers the collection and
uses the first family as the primary reading family. The selected filename is
saved with the book and accepted by document defaults and profiles. The font
file must be a regular file no larger than 12 MiB, like other user fonts.

KOReader lists `.ttc` files and enumerates their individual faces. Reader's
chooser currently has one row per file and cannot choose a collection's
individual faces; that finer selection remains open. The source test builds a
temporary collection from two licensed fixture fonts and exercises preview,
selection, failure rollback and reopening. It has not been run; stock runtime,
simulator and Kindle comparison remain pending. [KOReader font discovery](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/fontlist.lua#L105-L135) · [Reader font chooser](../src/app/typeface.rs)
