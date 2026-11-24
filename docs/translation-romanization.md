# Translation romanization

KOReader's pinned Translation settings offer **Show romanizations**. It defaults
off; when enabled, KOReader asks Google Translate for its romanization response
field and displays the source text, Latin transliteration when available, and
the translated text. The source option and renderer are in the pinned
[`translator.lua`](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/translator.lua#L405-L410)
and result assembly at [lines 3440–3483](https://github.com/koreader/koreader/blob/9192014d8bd82a91dc1012473be0f238dedfdb54/frontend/ui/translator.lua#L3440-L3483).

Reader exposes the saved switch at **Search → Translation settings**. It is
off for new installs and older lookup-history files. When on, a confirmed
Google Translate request includes `dt=rm`; the preview still sends no request.
The result sheet shows the source text returned by Google when available
(falling back to the submitted text), Google's Latin romanization if returned,
and the translation. MyMemory requests remain
unchanged, and missing romanization data does not fail a translation. The
preference is stored with Reader's local lookup settings, while each saved
request retains its effective choice for repeat lookups.

Automated coverage checks the default, saved toggle, compatibility with an old
request record, provider parameter, parsing of segmented response data, and
result layout. The screenshots use synthetic provider data:

- [Translation setting off](screenshots/translation-settings-romanization-off.png)
- [Translation setting on](screenshots/translation-settings-romanization-on.png)
- [Source, romanization, and translation result](screenshots/online-romanization-result.png)

The response-layout test does not contact Google. Provider availability and
romanization coverage depend on Google's response for a selected language pair.
