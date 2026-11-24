-- Run with the pinned native KOReader runtime and the authored options EPUB.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local Settings = require("luasettings")
G_reader_settings = Settings:wrap({})
local Device = require("device")
require("document/canvascontext"):init(Device)
local Typography = require("apps/reader/modules/readertypography")
local CreDocument = require("document/credocument")
local Cre = CreDocument:engineInit()
local UIManager = require("ui/uimanager")
local BB = require("ffi/blitbuffer")
local book, cache, font, output = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)
Cre.initHyphDict("./data/hyph/")

local width, height = 166, 720
local view = Cre.newDocView(width, height, 1)
view:setStyleSheet("./data/epub.css", "")
view:setVisiblePageCount(1, false)
view:setHeaderInfo(0)
view:setPageMargins(16, 16, 18, 18)
view:setFontFace("Noto Sans")
view:setFontSize(42)
assert(view:loadDocument(book, false))
local config = Settings:wrap({})
local doc = setmetatable({ _document = view, file = book }, { __index = CreDocument })
local typography = Typography:new{
    ui = {
        document = doc,
        doc_settings = config,
        menu = { registerToMainMenu = function() end },
        userhyph = { getMenuEntry = function() return {} end },
        handleEvent = function(_, event)
            if event.handler == "onUpdatePos" then view:renderDocument() end
        end,
    },
}
typography:onReadSettings(config)
typography:onPreRenderDocument()
view:renderDocument()
local hyphenation
for _, item in ipairs(typography.menu_table) do
    if item.text_func and item.text_func():match("^Hyphenation:") then
        hyphenation = item.sub_item_table
        break
    end
end
assert(hyphenation)
local function option(name)
    for _, item in ipairs(hyphenation) do
        local title = item.text or item.text_func and item.text_func()
        if title and title:sub(1, #name) == name then return item end
    end
    error(name)
end
local enabled = option("Enable hyphenation")
local trust = option("Trust soft hyphens")
local limits = option("Left/right minimal sizes")
local algorithmic = option("Algorithmic hyphenation")
local authored = option("Soft hyphens only")
local dictionary = option("Hyphenation dictionary:")

local function pointers(query)
    local hits = assert(view:findAllText(query, false, false, 20, true, 0, 255))
    local out = {}
    for i, hit in ipairs(hits) do
        local stem, length = hit["end"]:match("^(.-)%.(%d+)$")
        assert(stem and hit.start == stem .. ".0")
        out[i] = { stem = stem, length = tonumber(length) }
    end
    return out
end
local labs = pointers("laboratory")
local adv = pointers("adventure")[1]
assert(#labs == 4 and adv)
local function split(item)
    local first_y = view:getPosFromXPointer(item.stem .. ".0")
    for at = 1, item.length - 1 do
        local y = view:getPosFromXPointer(item.stem .. "." .. at)
        if y > first_y then return at end
    end
end
local function values(label, expected)
    local data = { split(labs[1]), split(labs[2]), split(labs[3]), split(adv), split(labs[4]) }
    local offsets = table.concat(data, ",")
    assert(offsets == expected, label .. ": " .. offsets)
    print(label, offsets)
    return data
end
local widget
UIManager.show = function(_, shown) widget = shown end

assert(enabled.checked_func() and dictionary.checked_func() and not trust.checked_func())
trust.hold_callback()
widget.choice2_callback()
assert(G_reader_settings:isTrue("hyph_trust_soft_hyphens") and not trust.checked_func())
algorithmic.hold_callback()
widget.choice2_callback()
assert(G_reader_settings:isTrue("hyph_force_algorithmic") and dictionary.checked_func())
values("dictionary", "5,4,4,5,5")
trust.callback()
assert(trust.checked_func())
values("trust", "3,4,4,5,5")
local frame = BB.new(width, height, BB.TYPE_BB8)
view:drawCurrentPage(frame, false, false, false, false)
frame:writePNG(output .. "/stock-hyphenation-trust.png")
limits.callback()
assert(widget.left_min == 1 and widget.right_min == 1)
assert(widget.left_max == 10 and widget.right_max == 10)
local left_default, right_default = widget.left_default, widget.right_default
widget.callback(5, 5)
assert(G_reader_settings:readSetting("hyph_left_hyphen_min") == 5)
assert(G_reader_settings:readSetting("hyph_right_hyphen_min") == 5)
assert(view:getIntProperty("crengine.hyphenation.left.hyphen.min") == 5)
values("limits", "3,6,6,5,5")
widget.callback(left_default, right_default)
assert(not G_reader_settings:has("hyph_left_hyphen_min"))
assert(not G_reader_settings:has("hyph_right_hyphen_min"))
algorithmic.callback()
assert(algorithmic.checked_func() and not dictionary.checked_func())
values("algorithmic", "3,4,4,5,4")
authored.callback()
assert(authored.checked_func() and not trust.enabled_func() and not algorithmic.checked_func())
values("authored", "3,6,6,5,6")
enabled.callback()
assert(not trust.enabled_func() and not authored.enabled_func() and not limits.enabled_func())
values("off", "7,6,6,5,6")
enabled.callback()
assert(authored.checked_func())
typography:onSaveSettings()
assert(config:isTrue("hyph_soft_hyphens_only") and config:isTrue("hyph_trust_soft_hyphens"))
view:close()
