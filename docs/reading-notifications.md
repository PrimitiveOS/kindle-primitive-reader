# Reading notifications

Open **Settings › Notifications** in Reader’s upper toolbar. Each switch
controls feedback from one kind of reading action. Reader saves the switches
across launches. Error messages, required confirmations, and reading alarms
keep their existing controls.

| Source | Default | Reader actions with feedback |
| --- | --- | --- |
| Bottom menu icons | Off | Screen orientation after the new view is accepted |
| Bottom menu toggles | Off | Font hinting and kerning, reading mode, and fixed-page fit |
| Bottom menu ± buttons | On | Fine line spacing and scan contrast |
| Bottom menu custom values | On | The accepted custom line-spacing value |
| Bottom menu progress bars | On | Line-spacing and scan-contrast presets |
| Gestures and profiles | On | Assigned previous/next location gestures and applied reading profiles |
| Other sources | Off | A previous-location jump from the reading menu |

The small notification at the top of the page clears after two seconds or
when you use the screen or buttons. A tap still reaches its original control.
Opening or canceling a custom-value editor leaves the history alone. A saved
change only adds a message if its source is on.

**Show past notifications** lists up to 20 messages with their local times,
newest first, for the current Reader session. Use the physical page buttons or
swipe to move through the list. Back returns to the notification switches.

## KOReader comparison

The pinned Mac replay ran the stock settings callbacks and the actual
`ReaderFont:onSetLineSpace` handler under all seven source flags. It checked
the default mask `0x3c`, independent switches, the two-second timeout and
gesture propagation to the view underneath. It also checked the empty and
populated history callbacks, the 20-message cap, and the stock always-show
route with all seven switches off. The seven switch rows have tap callbacks.
[Notification settings](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/elements/screen_notification_menu_table.lua#L22-L80) · [Stock widget](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/notification.lua#L25-L60) · [Lower-menu event sources](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/configdialog.lua#L615-L653)

KOReader can stack simultaneous toast widgets and use its text viewer’s Find
control. Reader uses one notification at a time and a paged history list.
The switches cover Reader’s mapped actions in the table. Further emitters
and foreground E Ink review remain open.

## Mac simulator

The simulator exercises the upper settings card, isolated switches and
saved state, the lower presets and custom editor, two-second expiration,
gesture and page-button routing, profiles, and a canceled and an accepted
viewport turn. It verifies source anchors after layout changes and returning
from history. The six notification tests passed with 54 neighboring reading
tests. The same run verified press feedback under a visible toast, unchanged
pixels outside its small rectangle, and the opening-profile deadline when a
toast expires.

[Reader switches](screenshots/reader-notification-settings.png) ·
[Reader line-spacing notice](screenshots/reader-notification-toast.png) ·
[Reader recent notifications](screenshots/reader-past-notifications.png) ·
[Stock history](screenshots/stock-past-notifications.png)

```sh
cargo test --locked --features kindle --lib reading_notifications
```

Set `READER_NOTIFICATION_SETTINGS_CAPTURE`,
`READER_NOTIFICATION_TOAST_CAPTURE`, and
`READER_NOTIFICATION_HISTORY_CAPTURE` to PNG paths to reproduce the Reader
frames. Run `scripts/koreader-reading-notifications-check.lua` in the pinned
Mac KOReader runtime with a scratch output directory for the stock replay.
