-- Run from the pinned native KOReader runtime with table-vertical-alignment.xhtml.
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

local settings = Settings:wrap({})
local changes = 0
local controller = require("apps/reader/modules/readerstyletweak"):new{
    ui = {
        menu = { registerToMainMenu = function() end },
        doc_settings = settings,
        handleEvent = function(_, event)
            assert(event.handler == "onApplyStyleSheet")
            changes = changes + 1
        end,
    },
}
controller:onReadSettings(settings)
local menu = {}
controller:addToMainMenu(menu)
local function find(items)
    for _, item in ipairs(items) do
        if item.tweak_id == "td_vertical_align_none" then return item end
        if type(item.sub_item_table) == "table" then
            local found = find(item.sub_item_table)
            if found then return found end
        end
    end
end
local option = assert(find(menu.style_tweaks.sub_item_table))
local labels = {
    "Tall one", "Tall two", "Tall three", "Top sample", "Mid sample", "Bottom sample",
    "Base sample", "Header sample", "Small capitals", "Small last", "Large capitals",
    "Header top", "Span bottom", "Upper first", "Upper last", "Lower first",
    "Lower last", "Span small start", "Span small end", "Early peer", "Later peer", "After table",
}
local w, h = 760, 930
local anchors
local function render(phase)
    local view = Cre.newDocView(w, h, 1)
    view:setIntProperty("crengine.render.block.rendering.flags", 0x03375131)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setStyleSheet("./data/epub.css", controller:getCssText())
    view:setPageMargins(32, 32, 32, 32)
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setFontFace("Noto Sans")
    view:setFontSize(20)
    assert(view:loadDocument(source, false))
    view:renderDocument()
    local source_positions, boxes = {}, {}
    view:gotoPage(1)
    for _, label in ipairs(labels) do
        local hits = assert(view:findAllText(label, false, false, 8, true, 0, 255))
        local hit = hits[1]
        assert(#hits == 1 and view:getTextFromXPointers(hit.start, hit["end"]) == label)
        assert(view:getPageFromXPointer(hit.start) == 1)
        local rect = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true)[1])
        boxes[label] = rect
        source_positions[#source_positions + 1] = hit.start
        print(phase, label, rect.x0, rect.y0, rect.y1)
    end
    local identity = table.concat(source_positions, ";")
    anchors = anchors or identity
    assert(identity == anchors)
    for _, label in ipairs({"Header sample", "Span bottom"}) do
        local rect = boxes[label]
        local link = view:getLinkFromPosition(rect.x0 + 4, math.floor((rect.y0 + rect.y1) / 2))
        assert(link:sub(-5) == "after")
    end
    local frame = BB.new(w, h, BB.TYPE_BB8)
    view:drawCurrentPage(frame, false, false, false, false)
    if phase ~= "restored" then
        frame:writePNG(output .. "/stock-table-vertical-" .. phase .. ".png", false)
    end
    local pixels = BB.tostring(frame)
    frame:free()
    view:close()
    return boxes, pixels
end

assert(not option.checked_func())
local before, plain = render("publisher")
option.callback()
assert(option.checked_func() and changes == 1)
local after, forced = render("baseline")
assert(plain ~= forced)
for _, label in ipairs(labels) do
    assert(before[label].x0 == after[label].x0)
end
assert(before["Mid sample"].y0 == 59 and after["Mid sample"].y0 == 37)
assert(before["Bottom sample"].y0 == 81 and after["Bottom sample"].y0 == 37)
assert(before["Header sample"].y0 == 81 and after["Header sample"].y0 == 81)
assert(before["Small capitals"].y0 == 125 and after["Small capitals"].y0 == 147)
assert(before["Large capitals"].y0 == after["Large capitals"].y0)
assert(before["After table"].y0 + 22 == after["After table"].y0)
assert(before["Span bottom"].y0 == before["Lower last"].y0)
assert(after["Span bottom"].y0 == after["Upper first"].y0)
assert(before["Span small start"].y0 == before["Early peer"].y0)
assert(after["Span small start"].y0 == after["Early peer"].y0)
option.callback()
assert(not option.checked_func() and changes == 2)
local _, restored = render("restored")
assert(restored == plain)
print("Pinned TD-only baseline override preserves TH declarations, links, positions and row paints")
