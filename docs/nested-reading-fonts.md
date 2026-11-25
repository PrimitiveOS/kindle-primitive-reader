# Nested custom reading fonts

Reader discovers TTF, OTF and TTC files in `reader/data/fonts` and its
subfolders, similar to KOReader's recursive font directory scan. The chooser
shows relative paths so files with the same name in different folders remain
distinguishable. Saved book and profile preferences retain that relative path;
existing preferences containing only a filename continue to work.

To bound directory work, discovery considers at most 256 entries in total and
font paths have at most eight components and 255 bytes. Fonts must be nonempty
regular files no larger than 12 MiB. Paths containing traversal, control
characters or backslashes are rejected; symlinked font files and directories
are not intentionally followed. Filesystem changes during discovery are not
atomic. Collection files currently select their first face; an individual-face
chooser remains future work.

The nested discovery, selection, persistence, rejected-path and symlink cases
have focused automated checks and an opt-in chooser screenshot capture, but
these checks and device behavior have not yet been verified for this change.
