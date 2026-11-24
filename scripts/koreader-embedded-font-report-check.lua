-- Exercise KOReader's real embedded font help against a two-chapter authored EPUB.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
local Device = require("device")
require("document/canvascontext"):init(Device)
local options = require("ui/data/creoptions")
local CreDocument = require("document/credocument")
local Cre = require("libs/libkoreader-cre")
local source, cache, noto, silk = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(noto)
Cre.registerFont(silk)

local option
for _, panel in ipairs(options) do
    for _, entry in ipairs(panel.options) do
        if entry.name == "embedded_fonts" then option = entry end
    end
end
assert(option and option.help_text_func and option.name_text_hold_callback)
local function inspect(enabled)
    local view = Cre.newDocView(1264, 1606, 1)
    view:setStyleSheet("./data/epub.css", "")
    view:setVisiblePageCount(1, false)
    view:setPageMargins(54, 54, 54, 54)
    view:setFontFace("Noto Sans")
    view:setFontSize(42)
    view:setIntProperty("crengine.doc.embedded.fonts.enabled", enabled)
    assert(view:loadDocument(source, false))
    view:renderDocument()
    local document = setmetatable({ _document = view }, { __index = CreDocument })
    local config = { embedded_css = 1, embedded_fonts = enabled }
    assert(option.enabled_func(config, document))
    local list = document:getEmbeddedFontList()
    assert(list ~= nil and list.Quarry == false)
    assert(list.Voyage == (enabled == 1))
    local help = assert(option.help_text_func(config, document))
    assert(help:find("\nQuarry  (not used)", 1, true))
    assert(help:find("\nVoyage", 1, true))
    assert((help:find("\nVoyage  (not used)", 1, true) ~= nil) == (enabled == 0))
    config.embedded_css = 0
    assert(not option.enabled_func(config, document))
    assert(option.help_text_func(config, document) == help)
    view:close()
end
inspect(1)
inspect(0)
print("Embedded font help: registered names, remote use, unused face, disabled use, and help while styles are off passed")
