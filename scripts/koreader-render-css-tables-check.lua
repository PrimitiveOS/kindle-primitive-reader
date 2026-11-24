-- Replay paired cells in the pinned renderer with and without table completion.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
local Device = require("device")
require("document/canvascontext"):init(Device)
local Typeset = require("apps/reader/modules/readertypeset")
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local source, cache, font, captures = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)

local width, height = 1264, 1606
local property = "crengine.render.block.rendering.flags"
local modes = { "legacy", "flat", "book", "web" }
local flags = { 0x00000000, 0x03030031, 0x03375131, 0x7FFFFFFF }
local phrases = {
    "Pier", "Buoy", "North", "South", "Ebb", "Flood", "Visible separator",
    "Light", "Dark", "Sand", "Rock", "Watch", "Dawn", "Rope", "Mast",
    "Hidden", "After harbor table",
}

local function word(view, phrase)
    for _, hit in ipairs(view:findAllText(phrase, false, false, 8, true, 0, 255) or {}) do
        if view:getTextFromXPointers(hit.start, hit["end"]) == phrase then
            assert(view:getPageFromXPointer(hit.start) == 1)
            return (view:getWordBoxesFromPositions(hit.start, hit["end"], true) or {})[1]
        end
    end
end
local function row(marks, a, b, right_x)
    assert(marks[a].x0 == 64 and marks[b].x0 == right_x, a .. "/" .. b)
    assert(marks[a].y0 == marks[b].y0, a .. "/" .. b)
    return marks[a].y0
end

for i, mode in ipairs(modes) do
    local view = Cre.newDocView(width, height, 1)
    local dispatched = 0
    local typeset = setmetatable({
        ui = {
            document = {
                setBlockRenderingFlags = function(_, mask) view:setIntProperty(property, mask) end,
            },
            handleEvent = function(_, event)
                assert(event.handler == "onUpdatePos")
                dispatched = dispatched + 1
            end,
        },
    }, { __index = Typeset })
    typeset:setBlockRenderingMode(i - 1)
    assert(dispatched == 1 and view:getIntProperty(property) == flags[i])
    view:setStyleSheet("./data/epub.css", "")
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(54, 54, 54, 54)
    view:setFontSize(40)
    view:setDefaultInterlineSpace(120)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setFontFace("Noto Sans")
    assert(view:loadDocument(source, false))
    view:renderDocument()
    assert(view:getPages() == 1)
    view:gotoPage(1)
    local marks = {}
    for _, phrase in ipairs(phrases) do marks[phrase] = word(view, phrase) end
    assert(not marks.Hidden)
    if mode == "legacy" then
        assert(not marks.Pier and not marks.Buoy and not marks.Watch and not marks.Dawn)
        assert(marks.Ebb.y0 < marks.Flood.y0 and marks.Light.y0 < marks.Dark.y0)
        row(marks, "North", "South", 469)
    else
        local right_x = mode == "web" and 469 or 642
        local first = row(marks, "Pier", "Buoy", right_x)
        local grouped = row(marks, "North", "South", right_x)
        local intrinsic = row(marks, "Ebb", "Flood", 146)
        local orphan = row(marks, "Light", "Dark", 642)
        local next_row = row(marks, "Sand", "Rock", 642)
        local split = row(marks, "Rope", "Mast", right_x)
        assert(first < grouped and grouped < intrinsic)
        assert(intrinsic < marks["Visible separator"].y0 and marks["Visible separator"].y0 < orphan)
        assert(orphan < next_row and next_row < marks.Watch.y0 and marks.Watch.y0 < marks.Dawn.y0)
        assert(marks.Dawn.y0 < split and split < marks["After harbor table"].y0)
    end
    local frame = BB.new(width, height, BB.TYPE_BB8)
    view:drawCurrentPage(frame, false, false, false, false)
    frame:writePNG(captures .. "/render-css-tables-stock-" .. mode .. ".png")
    frame:free()
    view:close()
    print(string.format("CSS table repair %s mask=%08x passed", mode, flags[i]))
end
