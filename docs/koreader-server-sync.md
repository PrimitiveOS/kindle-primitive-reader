# KOReader server sync

Reader has a separate, opt-in KOReader server provider for **PDF source pages**. Open the reading Tools icon grid → **KOReader sync**. Its action sheet stays over the current document; Done restores the prior reading view. Settings and the masked password form also keep the book underneath. The existing [WebDAV provider](progress-sync.md) retains its own account, records, conditional writes and remote-clear behavior.

Enter a server URL, username, password and device name in **KOReader sync settings**. **Sign in** checks an existing account; **Register account** creates one only when explicitly tapped. Then turn on KOReader sync. Editing settings, opening the sheet and ordinary reading do not contact a server. This initial provider has manual actions only.

- **Check server position** retrieves a PDF's saved source page and previews its page number/device. **Open page** navigates only after that choice; **Keep page** leaves the current page unchanged.
- **Push this PDF's position** explicitly replaces the server's position. A following read checks whether that page/device was retained; a competing update is shown rather than automatically overwritten again.
- **Sign out** cancels owned requests and removes this provider's username and saved key. Local positions, highlights, bookmarks and the WebDAV account stay intact.

The stock protocol has no conditional-write or remote-clear endpoint. Reader does not pretend that a preflight check prevents a later concurrent overwrite, and it does not send an empty position as a substitute for clearing. Reflow documents show why their actions are unavailable; no EPUB/native anchor is sent as a CREngine XPointer. Other fixed formats and reflow providers remain separate acceptance gates.

## Identity and storage

The default matching method is KOReader's sampled partial MD5 of the unchanged file. The optional filename method hashes the exact basename, including case and extension; different files with the same name can then match. Neither method replaces Reader's existing library/anchor identities or WebDAV resource names.

KOReader requires the entered password's MD5 as its `x-auth-key`. That saved key authenticates requests and is a password-equivalent secret. Reader stores the key, not the entered password, in its separate atomically replaced `kosync.json` with mode 0600 where supported. It excludes that account from reading-data backups and exports. The account, key and payload go to the worker on standard input, not process arguments. Redirects are not followed. Public servers require HTTPS; local-network HTTP requires its explicit setting, while loopback HTTP is available for authored tests. Cancel, sleep, USB handoff, leaving the provider and sign-out stop its owned worker. A delayed reply cannot switch another document or authorize a changed account.

PDF positions are validated one-based source pages. Opening one retains local zoom/crop preferences and a return location. Failed persistence restores the prior reading state. Metadata transmission is not offered in this first provider: title, authors, filename and document text are absent from its wire payload. The configured matching digest and current PDF source page are still transmitted by an explicit sync action.

## Evidence and remaining acceptance

The [source audit and execution plan](koreader-progress-interop-plan.md) records the exact pinned contracts and the separate CREngine XPointer gate. This implementation is independently written against those protocol facts; it imports no Lua implementation or rule dataset.

The unchanged pinned KOReader client ran against an authored loopback server on 2026-11-22. Eight actual requests checked registration, authorization, push and pull, both at the root and a custom `/compat/` path. Exact media type, hashed credential field, auth headers, string page and omitted metadata were checked. Nine actual `util.partialMD5` outputs covered empty/short/boundary data and all sampled offsets through a sparse 1 GiB authored file. They are retained as independent Rust regression vectors. The runtime hash inventory stayed unchanged; no external account or owner book was involved.

Local receipt: `<QA>/koreader-macos-reference/runs/kosync-protocol-20261122T112450Z/receipt.json`, SHA-256 `ea665aae07a8a3cd11ebd3bbc28345112ae6f3299ca3d1ee6f7869a862e5302f`. This oracle validates the stock contract; it does not by itself validate the new Rust worker or its PDF UI. Component CI, actual stock-client/Rust-worker two-client flow, desktop screenshot review and native Kindle acceptance are tracked separately. Native acceptance is pending while the Kindle is offline.
