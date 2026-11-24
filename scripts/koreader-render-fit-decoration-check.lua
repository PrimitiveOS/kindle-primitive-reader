-- Run with the pinned KOReader native runtime and an authored EPUB.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
local Device = require("device")
require("document/canvascontext"):init(Device)
local Typeset = require("apps/reader/modules/readertypeset")
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local source, cache, font, output = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)
local width, height = 1264, 1606
local cases = {
    {"Compass marker", {[221]=true}},
    {"Channel marker", {[211]=true}},
    {"Ripple marker", {[201]=true}},
    {"Measure marker", {[191]=true}},
    {"Boundary marker", {[181]=true,[51]=true,[85]=true}},
    {"Ledger marker", {[171]=true}},
    {"Beacon marker", {[161]=true,[68]=true}},
    {"Wide marker", {[151]=true}},
}
local expected = {
    {{54,1210,74}, {54,1210,74}, {54,1210,74}, {54,1210,111}, {54,1198,77}, {54,1210,66}, {54,1186,66}, {54,1210,74}},
    {{54,1210,74}, {54,1210,74}, {54,1210,74}, {54,1210,111}, {54,1198,77}, {54,1210,66}, {54,1186,66}, {54,1210,73}},
    {{441,823,461}, {516,748,536}, {532,732,552}, {416,848,473}, {54,463,77}, {471,792,483}, {766,1186,778}, {54,1210,73}},
    {{441,823,461}, {532,732,552}, {548,716,568}, {416,848,473}, {30,439,53}, {449,815,461}, {766,1186,778}, {54,1242,73}},
}
local function locate(view, phrase)
    local hit = assert((view:findAllText(phrase, false, false, 8, true, 0, 255) or {})[1], phrase)
    assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase)
    return hit
end
local function widest(frame, shades, top, bottom)
    local left, width_found, row = 0, 0, 0
    for y = top, bottom do
        local run = 0
        for x = 0, width - 1 do
            run = shades[frame:getPixel(x, y):getColor8().a] and run + 1 or 0
            if run > width_found then left, width_found, row = x + 1 - run, run, y end
        end
    end
    return left, width_found, row
end
for mode, label in ipairs({"Legacy", "Flat", "Book", "Web"}) do
    local view = Cre.newDocView(width, height, 1)
    local menu = setmetatable({ ui = {
        document = {setBlockRenderingFlags = function(_, flags) view:setIntProperty("crengine.render.block.rendering.flags", flags) end},
        handleEvent = function(_, event) assert(event.handler == "onUpdatePos") end,
    }}, { __index = Typeset })
    menu:setBlockRenderingMode(mode - 1)
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
    local frame = BB.new(width, height, BB.TYPE_BB8)
    local anchors = {}
    for index, case in ipairs(cases) do
        local anchor = locate(view, case[1])
        anchors[index] = anchor
        local page = view:getPageFromXPointer(anchor.start)
        view:gotoPage(page)
        local word = assert(view:getWordBoxesFromPositions(anchor.start, anchor["end"], true)[1])
        view:drawCurrentPage(frame, false, false, false, false)
        local left, band, row = widest(frame, case[2], word.y0, word.y0 + 4)
        print(string.format("FIT %s | %s | page=%d outer=%d..%d word=%d..%d", label, case[1], page, left, left + band, word.x0, word.x1))
        local want = expected[mode][index]
        assert(left == want[1] and left + band == want[2] and word.x0 == want[3], case[1])
        if index == 5 then
            for offset = 0, 4 do assert(frame:getPixel(left + offset, row):getColor8().a == 51) end
            for offset = 0, 6 do assert(frame:getPixel(left + band - 1 - offset, row):getColor8().a == 85) end
            local text_y = math.floor((word.y0 + word.y1) / 2)
            assert(view:getLinkFromPosition(math.max(60, word.x0 + 12), text_y) ~= "")
            assert(view:getLinkFromPosition(left + 2, text_y) == "")
        elseif index == 7 then
            assert(frame:getPixel(left, row):getColor8().a == 68)
            assert(frame:getPixel(left + band - 1, row):getColor8().a == 68)
            local complete_stroke_rows = 0
            for y = word.y0 - 15, word.y1 + 15 do
                local x, w = widest(frame, {[68]=true}, y, y)
                if x == left and w == band then complete_stroke_rows = complete_stroke_rows + 1 end
            end
            assert(complete_stroke_rows == 8)
        end
    end
    view:gotoPage(1)
    view:drawCurrentPage(frame, false, false, false, false)
    frame:writePNG(output .. "/fit-decoration-stock-" .. label:lower() .. ".png", false)
    for next_mode = 0, 3 do
        menu:setBlockRenderingMode(next_mode)
        view:renderDocument()
        for index, anchor in ipairs(anchors) do
            assert(view:getTextFromXPointers(anchor.start, anchor["end"]) == cases[index][1])
        end
    end
    frame:free()
    view:close()
end
Device:exit()
