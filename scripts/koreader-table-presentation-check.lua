-- Run with the pinned KOReader runtime, the XHTML specimen, cache, font and output.
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
local applications = 0
local controller = require("apps/reader/modules/readerstyletweak"):new{ ui = {
    menu = { registerToMainMenu = function() end },
    doc_settings = config,
    handleEvent = function(_, event)
        assert(event.handler == "onApplyStyleSheet")
        applications = applications + 1
    end,
} }
controller:onReadSettings(config)
local menu = {}
controller:addToMainMenu(menu)
local function find(items)
    for _, leaf in ipairs(items) do
        if leaf.tweak_id == "table_no_presentational_hints" then return leaf end
        if type(leaf.sub_item_table) == "table" then
            local found = find(leaf.sub_item_table)
            if found then return found end
        end
    end
end
local choice = assert(find(menu.style_tweaks.sub_item_table))
local width, height, margin = 780, 630, 26
local anchors
local function frame(name)
    local view = Cre.newDocView(width, height, 1)
    view:setIntProperty("crengine.render.block.rendering.flags", 0x03375131)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setStyleSheet("./data/epub.css", controller:getCssText())
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(margin, margin, margin, margin)
    view:setFontFace("Noto Sans")
    view:setFontSize(32)
    assert(view:loadDocument(source, false))
    view:renderDocument()
    local pointers, boxes = {}, {}
    for index, phrase in ipairs({ "North dock", "Mooring rings", "Wind", "Morning watch", "Calm", "CSS table" }) do
        local hits = assert(view:findAllText(phrase, false, false, 16, true, 0, 255))
        assert(#hits == 1, phrase)
        local hit = hits[1]
        assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase)
        pointers[index] = hit.start
        local page = view:getPageFromXPointer(hit.start)
        assert(page == view:getPageFromXPointer(pointers[1]), phrase .. " reached another page")
        view:gotoPage(page)
        local parts = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true))
        boxes[index] = parts[1]
        if index == 2 then
            assert(view:getLinkFromPosition(parts[1].x0 + 4, parts[1].y0 + 6) == "#end")
        end
    end
    local current = table.concat(pointers, ",")
    anchors = anchors or current
    assert(current == anchors)
    local page = BB.new(width, height, BB.TYPE_BB8)
    view:drawCurrentPage(page, false, false, false, false)
    local counts = { [204] = 0, [230] = 0 }
    for y = margin, height - margin - 1 do
        for x = margin, width - margin - 1 do
            local gray = page:getPixel(x, y):getColor8().a
            if counts[gray] then counts[gray] = counts[gray] + 1 end
        end
    end
    page:writePNG(output .. "/stock-table-presentation-" .. name .. ".png", false)
    page:free()
    view:close()
    return boxes, counts
end
assert(not choice.checked_func())
local before, shown = frame("attributes")
choice.callback()
assert(choice.checked_func() and applications == 1)
local after, hidden = frame("stylesheet")
assert(before[1].x0 > after[1].x0 + 50)
assert(before[2].x0 > after[2].x0 + 35)
assert(before[6].x0 == after[6].x0)
assert(shown[204] > hidden[204] + 2000)
assert(shown[230] > 800 and hidden[230] > 800)
print("HTML align, tracks and row fill suppressed; CSS-only alignment and backgrounds, source spans, and link preserved")
