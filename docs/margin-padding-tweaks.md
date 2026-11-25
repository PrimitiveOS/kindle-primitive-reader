# Publisher margin and padding tweaks

Reader exposes the KOReader style-tweak choice for ignoring publisher body
margins. It also offers choices for removing vertical margins
from all elements or only paragraphs and list items. It also offers
paragraph-only horizontal and vertical padding suppression alongside the
existing all-element padding choices. Each all-element choice is mutually
exclusive with its paragraph-only counterpart, using the existing global and
per-book style settings.

The declarations follow the pinned [KOReader style tweaks](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/data/css_tweaks.lua#L42-L96).
They use Reader's existing supported CSS cascade. A layout mode that already
ignores a publisher margin or padding may show no visible change. This change
has not yet been built, compared in the simulator, or validated on Kindle.
