-- Run with the pinned native KOReader runtime and the optical margins EPUB.
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

local width, height = 1264, 1606
local view = Cre.newDocView(width, height, 1)
view:setStyleSheet("./data/epub.css", "")
view:setVisiblePageCount(1, false)
view:setHeaderInfo(0)
view:setPageMargins(54, 54, 54, 54)
view:setFontFace("Noto Sans")
view:setFontSize(42)
assert(view:loadDocument(book, false))
local saved = Settings:wrap({})
local document = setmetatable({ _document = view, file = book }, { __index = CreDocument })
local controller = Typography:new{
    ui = {
        document = document,
        doc_settings = saved,
        menu = { registerToMainMenu = function() end },
        userhyph = { getMenuEntry = function() return {} end },
        handleEvent = function(_, event)
            if event.handler == "onUpdatePos" then view:renderDocument() end
        end,
    },
}
controller:onReadSettings(saved)
controller:onPreRenderDocument()
view:renderDocument()
local option
for _, choice in ipairs(controller.menu_table) do
    if choice.text == "Hanging punctuation" then option = choice end
end
assert(option and not option.checked_func())
local shown
UIManager.show = function(_, widget) shown = widget end
option.hold_callback()
shown.choice2_callback()
assert(G_reader_settings:isTrue("floating_punctuation") and not option.checked_func())

local cases = {"Harbor opening marker", "Onshore closing marker", "Centered marker",
               "Fixed marker", "Justified marker", "before a second line starts",
               "بالعالم", "The destination"}
local points = {}
for i, query in ipairs(cases) do
    local match = assert((view:findAllText(query, false, false, 8, true, 0, 255) or {})[1], query)
    local y, x = view:getPosFromXPointer(match.start)
    points[i] = {start=match.start, finish=match["end"], x=x, y=y}
end
option.callback()
assert(option.checked_func())
assert(view:getIntProperty("crengine.style.floating.punctuation.enabled") == 1)
for i, point in ipairs(points) do
    local y, x = view:getPosFromXPointer(point.start)
    assert(y == point.y)
    assert(view:getTextFromXPointers(point.start, point.finish) == cases[i])
    points[i].shift = x - point.x
    print(cases[i], points[i].shift)
end
assert(points[1].shift < 0 and points[2].shift > 0)
assert(points[3].shift == 0 and points[4].shift == 0 and points[5].shift < 0)
assert(points[6].shift == 0 and points[7].shift > 0)
controller:onSaveSettings()
assert(saved:readSetting("floating_punctuation") == 1)
local frame = BB.new(width, height, BB.TYPE_BB8)
view:drawCurrentPage(frame, false, false, false, false)
frame:writePNG(output .. "/stock-hanging-punctuation.png")
option.callback()
assert(not option.checked_func())
for _, point in ipairs(points) do
    local y, x = view:getPosFromXPointer(point.start)
    assert(x == point.x and y == point.y)
end
print("Stock option toggles, global default, source pointers and visual edges verified")
view:close()
