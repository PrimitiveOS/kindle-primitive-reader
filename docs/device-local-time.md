# Reader uses the Kindle time zone

Reader 0.1.27 reads the same time-zone file as Launcher. On this Oasis, the
firmware's file has explicit transitions and an empty rule footer. A stock
Kindle clock read reported UTC+07:00 and a static musl probe reported UTC for
the same instant. The shared tool reads the 64-bit transition table, keeps the
last known firmware offset when there is no recurring rule, and uses the
platform clock for recurring rules beyond the table. Calendar queries reuse
one snapshot for all visits in the query. [TZif layout](https://www.rfc-editor.org/rfc/rfc9636.html#section-3)

| Workflow | Behavior |
| --- | --- |
| Reading footer and top header | Display the device clock. Custom footer date and time tokens share the same local date and time. The footer reads the clock only when a selected item uses it. |
| Read alarm | The picker opens at the current local hour and minute. Start resolves the next occurrence of the selected time. In a repeated hour it can choose the second occurrence when the first has passed. A skipped time advances by the clock change. The reading alarm still uses a monotonic timer after it starts. |
| Saved marks | Display and export the device's local date and time. Imported dates without a zone use the device zone, choose the earlier instant in a repeated hour, and reject skipped times. Dates with an explicit offset preserve their absolute instant. |
| Reading history and vocabulary | Group recorded UTC instants by the local reading day. Custom day starts and whole-day skips use the same time-zone snapshot. Vocabulary review dates follow the device calendar. |

Saved marks and journals retain their stored UTC instants. The update cannot
reconstruct a source wall time if an older import already interpreted it using
the wrong offset.

## Comparison with KOReader

KOReader reads custom footer date and time tokens from its local clock and
imports its annotation timestamps as local calendar values. Its read timer
chooses a local clock time, rolls to tomorrow if that time has passed, then
runs a monotonic timer. The Kindle-wide date and time controls belong to
PrimitiveOS Launcher. Reader keeps 24-hour time with a leading zero. KOReader
can use an unpadded hour and its global 12-hour option. Reader's optional
automatic refresh can schedule a minute update for custom clock tokens. In
both readers the seconds token shows the seconds at render time.
[Footer tokens](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/filemanager/filemanagerbookinfo.lua#L1183-L1187) ·
[Time formatting](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/datetime.lua#L239-L265) ·
[Read timer](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/plugins/readtimer.koplugin/main.lua#L423-L464) ·
[Annotation timestamp](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerannotation.lua#L506-L517)

## Verification

Tests exercise 23- and 25-hour days, a fractional UTC offset, a reading day
starting at 04:00, skipped and repeated hours, and annotation timestamp round
trips. With the locked Kindle feature set, macOS passed 983 Reader tests,
the application binary test, 14 EPUB package tests and four layout probes.
A separate New York test passed both daylight saving boundaries. An independent
comparison against Python zoneinfo matched 156 offsets and local-time round
trips using the Oasis's actual TZif file.

On the Oasis 3, the footer tokens, alarm picker, saved-mark dates and a synthetic
calendar visit all matched the Kindle's own `date` output for the same instant.
The native suite passed 987 Reader tests, 14 EPUB package tests and four layout
probes. Both Reader suites skipped 62 tests that require separate inputs.
The native reader test executable SHA-256 was
`0ea42ad2dc4e99e34eb965ca93147c3103d11443a3b760393913d29ebc9790b2`.
The captures use authored documents and synthetic reading history.
[Oasis footer](screenshots/device-local-footer-clock-oasis.png) ·
[Oasis alarm picker](screenshots/device-local-alarm-oasis.png)

[Reading footer](screenshots/device-local-footer-clock.png) ·
[Set alarm](screenshots/device-local-alarm.png) ·
[Reading calendar](screenshots/device-local-reading-calendar.png) ·
[Reading day](screenshots/device-local-reading-day.png)

## Release and installation

[Reader 0.1.27](https://github.com/PrimitiveOS/kindle-primitive-reader/releases/tag/v0.1.27)
passed the GitHub release workflow. Its ARMHF ZIP contains both project licences
and matches SHA-256
`be7545c6fce8554e1b0441f4ae1eb0acaabaefb847c0a566d62167d0c1572f5e`
(21,243,208 bytes). The Oasis installed this package through Store's verified
install transaction. The installed executable SHA-256 is
`e2735705aa1e61e195fb507019a349cf140341f420daf7b42c1b763771887a80`.
The exact installed binary opened, advanced, and resumed an existing EPUB and
PDF through the normal picker in scratch profiles. Both source hashes matched
before and after. First frames took 1.49 and 18.38 seconds respectively. The
samples were rendered offscreen while Primitive Browser was in the foreground.
[Store listing](https://primitiveos.github.io/kindle-primitive-catalog/v1/apps/primitive-reader.json)
