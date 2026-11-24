-- Compare the stock Embedded Fonts control on authorized KF8 input, using only the pinned local runtime.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local Settings = require("luasettings")
G_reader_settings = Settings:wrap({})
local Device = require("device")
require("document/canvascontext"):init(Device)
local Cre = require("libs/libkoreader-cre")
local CreDocument = require("document/credocument")
local options = require("ui/data/creoptions")
local cache, font = assert(arg[1]), assert(arg[2])
assert(arg[3], "supply at least one KF8 specimen")
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)
local option
for _, panel in ipairs(options) do
    for _, item in ipairs(panel.options) do
        if item.name == "embedded_fonts" then option = item end
    end
end
assert(option and option.enabled_func and option.help_text_func)
local inspected = 0
for index = 3, #arg do
    local view = Cre.newDocView(600, 800, 1)
    view:setStyleSheet("./data/epub.css", "")
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(30, 30, 30, 30)
    view:setFontFace("Noto Sans")
    view:setFontSize(32)
    view:setIntProperty("crengine.doc.embedded.fonts.enabled", 1)
    assert(view:loadDocument(arg[index], false), "reference did not open the KF8 specimen")
    view:renderDocument()
    assert(view:getPages() >= 1)
    local document = setmetatable({ _document = view }, { __index = CreDocument })
    local config = { embedded_css = 1, embedded_fonts = 1 }
    assert(not option.enabled_func(config, document))
    assert(option.help_text_func(config, document) == nil)
    assert(next(document:getEmbeddedFontList()) == nil)
    view:close()
    inspected = inspected + 1
end
print(string.format("KOReader opened %d KF8 documents and exposed no embedded-font details", inspected))
