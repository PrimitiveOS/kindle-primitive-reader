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
trips. Native tests compare the footer tokens, alarm picker, saved-mark dates
and a synthetic calendar visit with the Kindle's own `date` output for the
same instant. The simulator captures use authored documents and a synthetic
reading history.

[Reading footer](screenshots/device-local-footer-clock.png) ·
[Set alarm](screenshots/device-local-alarm.png) ·
[Reading calendar](screenshots/device-local-reading-calendar.png) ·
[Reading day](screenshots/device-local-reading-day.png)
