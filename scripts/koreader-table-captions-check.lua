-- Run from the pinned Mac KOReader runtime.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local Settings = require("luasettings")
G_reader_settings = Settings:wrap({})
require("document/canvascontext"):init(require("device"))
local cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local fixture, cache, font, output = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
cre.initCache(cache, 0, true, 1)
cre.registerFont(font)

local state = Settings:wrap({})
local controller = require("apps/reader/modules/readerstyletweak"):new{
    ui = {
        menu = { registerToMainMenu = function() end },
        doc_settings = state,
        handleEvent = function(_, event) assert(event.handler == "onApplyStyleSheet") end,
    },
}
controller:onReadSettings(state)
local menu = {}
controller:addToMainMenu(menu)
local function find(items)
    for _, item in ipairs(items) do
        if item.tweak_id == "table_no_presentational_hints" then return item end
        if type(item.sub_item_table) == "table" then
            local found = find(item.sub_item_table)
            if found then return found end
        end
    end
end
local hints = assert(find(menu.style_tweaks.sub_item_table))
local phrases = {
    "Late caption", "North berth", "North tide",
    "Attribute caption", "East berth", "East tide",
    "Author caption", "South berth", "South tide",
    "Override caption", "West berth", "West tide",
    "End of quay log.",
}
local canonical
local function capture(name, html_ignored)
    local width, height = 640, 880
    local view = cre.newDocView(width, height, 1)
    view:setIntProperty("crengine.render.block.rendering.flags", 0x03375131)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setStyleSheet("./data/epub.css", controller:getCssText())
    view:setPageMargins(40, 40, 40, 40)
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setFontFace("Noto Sans")
    view:setFontSize(22)
    assert(view:loadDocument(fixture, false))
    view:renderDocument()
    local y, pointers, boxes = {}, {}, {}
    for i, phrase in ipairs(phrases) do
        local matches = assert(view:findAllText(phrase, false, false, 16, true, 0, 255))
        assert(#matches == 1)
        local hit = matches[1]
        assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase)
        assert(view:getPageFromXPointer(hit.start) == 1)
        pointers[i] = hit.start
        y[i] = view:getPosFromXPointer(hit.start)
        boxes[i] = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true))[1]
    end
    local identity = table.concat(pointers, "/")
    canonical = canonical or identity
    assert(identity == canonical)
    assert(y[1] < y[2] and y[2] < y[3])
    assert(html_ignored and y[4] < y[5] or not html_ignored and y[6] < y[4])
    assert(y[8] < y[9] and y[9] < y[7])
    assert(y[10] < y[11] and y[11] < y[12] and y[12] < y[13])
    view:gotoPage(1)
    local page = BB.new(width, height, BB.TYPE_BB8)
    view:drawCurrentPage(page, false, false, false, false)
    for _, i in ipairs({1, 4, 7, 10}) do
        local rect = boxes[i]
        local center_y = math.floor((rect.y0 + rect.y1) / 2)
        assert(page:getPixel(230, center_y):getColor8().a == 221)
        local stroke_x = i == 10 and 40 or 43
        assert(page:getPixel(stroke_x, center_y):getColor8().a == 102)
    end
    local openers = { html_ignored and 4 or 5, 8, 10 }
    local closers = { 3, html_ignored and 6 or 4, 7 }
    for i, start in ipairs(openers) do
        local white = 0
        for row = boxes[closers[i]].y1, boxes[start].y0 - 1 do
            if page:getPixel(40, row):getColor8().a == 255 then white = white + 1 end
        end
        assert(white == 8, "Inter-table margin: " .. white)
    end
    page:writePNG(output .. "/stock-table-captions-" .. name .. ".png", false)
    local pixels = BB.tostring(page)
    page:free()
    view:close()
    print(name, table.concat(y, ","))
    return pixels
end
assert(not hints.checked_func())
local original = capture("publisher", false)
hints.callback()
assert(hints.checked_func())
assert(capture("without-html-hints", true) ~= original)
hints.callback()
assert(not hints.checked_func() and capture("restored", false) == original)
