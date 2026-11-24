-- Run in the pinned native KOReader runtime with the authored EPUB exported by
-- READER_TYPOGRAPHY_LANGUAGE_FIXTURE, an empty cache, Noto Sans, and an output dir.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local Settings = require("luasettings")
G_reader_settings = Settings:wrap({})
local Device = require("device")
require("document/canvascontext"):init(Device)

local Typography = require("apps/reader/modules/readertypography")
local CreDocument = require("document/credocument")
local Cre = require("libs/libkoreader-cre")
local UIManager = require("ui/uimanager")
local BB = require("ffi/blitbuffer")
local md5 = require("ffi/sha2").md5
local source, cache, font, output = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)
Cre.initHyphDict("./data/hyph/")

local width, height = 1264, 1606
local view = Cre.newDocView(width, height, 1)
view:setStyleSheet("./data/epub.css", "")
view:setVisiblePageCount(1, false)
view:setHeaderInfo(0)
view:setPageMargins(54, 54, 54, 54)
view:setFontFace("Noto Sans")
view:setFontSize(42)
assert(view:loadDocument(source, false))
local publication_language = assert(view:getDocumentProps().language)
assert(publication_language == "eng")
local frame = BB.new(width, height, BB.TYPE_BB8)
local redraws = 0
local config = Settings:wrap({})
local doc = setmetatable({
    _document = view,
    file = source,
    getProps = function() return { language = publication_language } end,
}, { __index = CreDocument })
local state = Typography:new{
    ui = {
        document = doc,
        doc_settings = config,
        menu = { registerToMainMenu = function() end },
        userhyph = { getMenuEntry = function() return {} end },
        handleEvent = function(_, event)
            if event.handler == "onUpdatePos" then
                view:renderDocument()
                redraws = redraws + 1
            end
        end,
    },
}
local function saved()
    local book = Settings:wrap({})
    state.ui.doc_settings = book
    state:onSaveSettings()
    return book
end
local function check(settings, publication, expected, respect)
    state.allow_doc_lang_tag_override = nil
    doc.getProps = function() return { language = publication } end
    state:onReadSettings(Settings:wrap(settings))
    state:onPreRenderDocument()
    assert(state.text_lang_tag == expected, tostring(state.text_lang_tag))
    assert(Cre.getTextLangStatus() == expected)
    assert(state.text_lang_embedded_langs == respect)
    assert(state.book_lang_tag == state:fixLangTag(publication))
end

local shown
UIManager.show = function(_, widget) shown = widget end
local tags = Typography.getLangTags()
assert(#tags == 54 and tags[1] == true)
local function option(tag)
    for i, candidate in ipairs(tags) do
        -- onPreRenderDocument has inserted the publication row at the front.
        if tag == candidate then return state.language_submenu[i] end
    end
end
check({}, publication_language, "en-US", true)
local toggle
for _, item in ipairs(state.menu_table) do
    if item.text == "Respect embedded lang tags" then toggle = item end
end
assert(toggle and toggle.checked_func())
assert(option("fr"))
option("fr").hold_callback()
assert(shown.choice2_callback)
shown.choice2_callback()
assert(G_reader_settings:readSetting("text_lang_fallback") == "fr")
assert(not G_reader_settings:has("text_lang_default"))
check({}, "en-GB", "en-GB", true)
check({}, nil, "fr", true)
-- Use the global setting files on this restart-equivalent path. Final rendering
-- uses the actual Lua actions and native property bridge below.
G_reader_settings:saveSetting("text_lang_default", "en-US")
G_reader_settings:delSetting("text_lang_fallback")
check({}, "en-GB", "en-US", true)
toggle.hold_callback()
assert(shown.choice1_callback)
shown.choice1_callback()
check({}, "vi", "en-US", false)
check({ text_lang = "vi", text_lang_embedded_langs = true }, "en-GB", "vi", true)
G_reader_settings:delSetting("text_lang_default")
G_reader_settings:delSetting("text_lang_embedded_langs")
check({}, "vi", "vi", true)

assert(Cre.softHyphenateText("en-US", "laboratory") ~= Cre.softHyphenateText("en-GB", "laboratory"))
state:onSetTypographyLanguage("en-US")
assert(Cre.getTextLangStatus() == "en-US")
local match = assert((view:findAllText("laboratory", false, false, 8, true, 0, 255) or {})[1])
state:onSetTypographyLanguage("en-GB")
assert(Cre.getTextLangStatus() == "en-GB")
assert(view:getTextFromXPointers(match.start, match["end"]) == "laboratory")
local old_redraws = redraws
toggle.callback()
assert(redraws == old_redraws + 1 and not toggle.checked_func())
view:drawCurrentPage(frame, false, false, false, false)
frame:writePNG(output .. "/stock-typography-language-uk.png", false)
local book = saved()
assert(book:readSetting("text_lang") == "en-GB")
assert(book:readSetting("text_lang_embedded_langs") == false)
print("53 languages; alias and metadata precedence; default/fallback hold; embedded hold and toggle; saved book settings; canonical passage stable")
print("native UK/US hyphenation differs; UK frame: " .. md5(BB.tostring(frame)))
view:close()
