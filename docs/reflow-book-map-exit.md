# Return from the reflow Book Map

The reflow Book Map has a visible **Return to book** control in its footer. It closes nested Book Map and Page Browser views and returns to the existing reading position and viewport without opening the browsed chapter. **Back** keeps its one-level behavior. Notices use the reserved space above the control.

This follows the intent of KOReader's [close-parent behavior](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/widget/bookmapwidget.lua#L1502-L1511). Reader presents a visible touch control rather than the original keyboard shortcut. Layout, navigation and Kindle behavior still require validation.
