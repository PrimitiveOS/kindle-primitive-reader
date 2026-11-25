-- Run from the pinned native KOReader runtime with the authored table style fixture.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local Settings = require("luasettings")
G_reader_settings = Settings:wrap({})
require("document/canvascontext"):init(require("device"))
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local source, cache, font, output = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)

local config = Settings:wrap({})
local events = 0
local controller = require("apps/reader/modules/readerstyletweak"):new{
    ui = {
        menu = { registerToMainMenu = function() end },
        doc_settings = config,
        handleEvent = function(_, event)
            assert(event.handler == "onApplyStyleSheet")
            events = events + 1
        end,
    },
}
controller:onReadSettings(config)
local menu = {}
controller:addToMainMenu(menu)
local function find(items, id)
    for _, item in ipairs(items) do
        if item.tweak_id == id then return item end
        if type(item.sub_item_table) == "table" then
            local result = find(item.sub_item_table, id)
            if result then return result end
        end
    end
end

local stripes = assert(find(menu.style_tweaks.sub_item_table, "table_row_odd_even"))
local borders = assert(find(menu.style_tweaks.sub_item_table, "table_force_border"))
assert(not stripes.checked_func() and not borders.checked_func())

local phrases = {
    "Head row", "Body first", "Own background", "Body third", "Body fourth",
    "New group", "Foot row", "Direct caption", "Direct first", "Direct second",
    "Publisher left", "Publisher right", "Single column", "Second solo",
}
local width, height = 640, 880
local anchors
local function render(name)
    local view = Cre.newDocView(width, height, 1)
    view:setIntProperty("crengine.render.block.rendering.flags", 0x03375131)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setStyleSheet("./data/epub.css", controller:getCssText())
    view:setPageMargins(40, 40, 40, 40)
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setFontFace("Noto Sans")
    view:setFontSize(22)
    assert(view:loadDocument(source, false))
    view:renderDocument()
    local references, rects = {}, {}
    for index, phrase in ipairs(phrases) do
        local hits = assert(view:findAllText(phrase, false, false, 16, true, 0, 255))
        assert(#hits == 1 and view:getTextFromXPointers(hits[1].start, hits[1]["end"]) == phrase)
        references[index] = hits[1].start
        assert(view:getPageFromXPointer(hits[1].start) == 1, phrase)
        rects[index] = assert(view:getWordBoxesFromPositions(hits[1].start, hits[1]["end"], true))[1]
    end
    local identity = table.concat(references, ";")
    anchors = anchors or identity
    assert(identity == anchors)
    view:gotoPage(1)
    local pixels = BB.new(width, height, BB.TYPE_BB8)
    view:drawCurrentPage(pixels, false, false, false, false)
    pixels:writePNG(output .. "/stock-table-style-" .. name .. ".png", false)
    local summaries = {}
    for index, rect in ipairs(rects) do
        local y = math.floor((rect.y0 + rect.y1) / 2)
        local sample_x = rect.x0 + 164
        local gray = pixels:getPixel(sample_x, y):getColor8().a
        local edge = pixels:getPixel(40, y):getColor8().a
        summaries[index] = { gray = gray, edge = edge, y = y, x = rect.x0 }
        print(name, phrases[index], rect.x0, y, gray)
    end
    local separate = rects[12]
    assert(pixels:getPixel(separate.x0 - 13, summaries[12].y):getColor8().a == 255)
    local frame = BB.tostring(pixels)
    pixels:free()
    view:close()
    return summaries, frame
end

local original, original_frame = render("original")
stripes.callback()
assert(stripes.checked_func() and not borders.checked_func() and events == 1)
local shaded, shaded_frame = render("striped")
assert(shaded_frame ~= original_frame)
local expected = { 221, 238, 170, 238, 204, 238, 238, 238, 238, 204, 238, 238, 238, 204 }
for index, row in ipairs(shaded) do
    assert(row.x == original[index].x and row.y == original[index].y, phrases[index])
    assert(row.gray == expected[index], phrases[index] .. ": " .. row.gray)
end
assert(original[2].gray == 136 and original[3].gray == 170)
borders.callback()
assert(stripes.checked_func() and borders.checked_func() and events == 2)
local combined, both = render("combined")
assert(both ~= shaded_frame)
for _, index in ipairs({ 1, 2, 4, 5, 6, 7, 9, 10, 13, 14 }) do
    assert(original[index].edge == 255 and combined[index].edge == 0, phrases[index])
end
assert(original[8].gray == combined[8].gray and original[8].edge == combined[8].edge)
assert(original[11].edge == 102 and combined[11].edge == 102)
stripes.callback()
assert(not stripes.checked_func() and borders.checked_func() and events == 3)
local _, forced = render("borders")
assert(forced ~= original_frame and forced ~= both)
borders.callback()
assert(not stripes.checked_func() and not borders.checked_func() and events == 4)
local _, restored = render("restored")
assert(restored == original_frame)
