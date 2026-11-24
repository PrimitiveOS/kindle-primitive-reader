# Cycle selected footer items

Open **Reading status bar** and tap **Status text** until **Custom items**
appears. Enable **Cycle custom items**. Tap the status bar to show each selected item in the
configured order. After the last item, the footer hides until the next tap.
In overlap mode, tap **Status** to bring it back. A clock or battery refresh
runs while the active item needs it. Named footer presets retain the cycle
and its selected item.

The switch starts off. The usual **Chapter and page** status shows
`Chapter x of y - Page m of n`, the horizontal progress track and Exit.
The **Lock status bar taps** switch also locks custom item cycling.
[Reading status bar](screenshots/footer-cycle-settings.png) · [Single item on the reading page](screenshots/footer-cycle-book.png)

KOReader cycles enabled footer modes, includes an off step and skips its
dynamic filler. Reader also skips enabled items with unavailable text and
follows the chosen Reader item order. The pinned callback check runs the
ordinary stock tap path with a filtered item list and tap lock. Reader's
EPUB and PDF tests cover ordering, the hidden step, resume, presets, footer
geometry and clock refresh when only the active item needs it.
[KOReader footer callbacks](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/apps/reader/modules/readerfooter.lua#L2469-L2504) · [Stock callback check](../scripts/koreader-footer-item-cycle-check.lua)
