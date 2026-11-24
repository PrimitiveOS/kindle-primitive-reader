-- Compare fitted paragraph height and overflow using pinned KOReader.
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
local dpi = tonumber(arg[5]) or 96
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)
local width, height = 1264, 1606
local cases = {
    {"Content bay", 221}, {"Border bay", 204}, {"Narrow basin", 187},
    {"North channel", 170}, {"Quiet water", 153}, {"Current line", 136},
    {"Twin banks", 119}, {"Lower bank", 119}, {"Deep reach", 221},
    {"Open shore", 204}, {"Tidal fall", 187}, {"Bright harbor", 136}, {"Far headland", 170, 68},
    {"After the headland", 255, 68}, {"Last headland", 153, 68},
    {"After the last headland", 255},
}
local book_heights = {109,109,109,109,109,109,173,173,126,109,109,109,110,0,110,0}
local web_heights = {225,180,145,200,181,109,173,173,205,205,109,184,1498,0,1498,0}
local book_pages = {1,1,1,1,1,1,1,1,2,2,2,2,3,3,4,5}
local web_pages = {1,1,1,1,1,1,1,1,2,2,2,2,3,4,5,7}

local function locate(view, phrase)
    local hit = assert((view:findAllText(phrase, false, false, 8, true, 0, 255) or {})[1], phrase)
    assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase)
    return hit
end

local function box_bounds(frame, word, gray, border)
    local left, right = 0, 0
    for y = word.y0, word.y0 + 4 do
        local run = 0
        for x = 0, width - 1 do
            run = frame:getPixel(x, y):getColor8().a == gray and run + 1 or 0
            if run > right - left then left, right = x + 1 - run, x + 1 end
        end
    end
    local x = right - 8
    local function pixel(y) return frame:getPixel(x, y):getColor8().a end
    local top, bottom = word.y0, word.y0
    while top > 0 and pixel(top - 1) == gray do top = top - 1 end
    while bottom < height and pixel(bottom) == gray do bottom = bottom + 1 end
    local leading, trailing = border or 51, border or 85
    while top > 0 and pixel(top - 1) == leading do top = top - 1 end
    while bottom < height and pixel(bottom) == trailing do bottom = bottom + 1 end
    return left, right, top, bottom, pixel(top), pixel(bottom - 1)
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
    view:setIntProperty("crengine.render.dpi", dpi)
    view:setFontFace("Noto Sans")
    assert(view:loadDocument(source, false))
    view:renderDocument()
    local frame = BB.new(width, height, BB.TYPE_BB8)
    local anchors, pages = {}, {}
    for i, case in ipairs(cases) do
        local anchor = locate(view, case[1])
        anchors[i] = anchor
        local page = view:getPageFromXPointer(anchor.start)
        pages[i] = page
        if dpi == 96 then
            assert(page == (label == "Web" and web_pages or book_pages)[i], case[1])
        end
        view:gotoPage(page)
        local word = assert(view:getWordBoxesFromPositions(anchor.start, anchor["end"], true)[1])
        view:drawCurrentPage(frame, false, false, false, false)
        if case[2] ~= 255 then
            local left, right, top, bottom, lead, tail = box_bounds(frame, word, case[2], case[3])
            print(string.format("FIT H %s | %s | page=%d x=%d..%d y=%d..%d box=%d word=%d..%d ends=%d/%d", label, case[1], page, left, right, top, bottom, bottom - top, word.y0, word.y1, lead, tail))
            if dpi == 96 then
                assert(bottom - top == (label == "Web" and web_heights or book_heights)[i], case[1])
                assert(lead == (case[3] or 51), case[1])
                local expected_tail = label == "Web" and case[3] and case[2] or (case[3] or 85)
                assert(tail == expected_tail, case[1])
                local leading = case[3] and 21 or 20
                if i == 8 then leading = leading + 64 end
                assert(word.y0 - top == leading, case[1])
                if i == 1 then assert(view:getLinkFromPosition(word.x0 + 15, word.y0 + 10) ~= "") end
            elseif dpi == 144 and i == 12 then
                assert(bottom - top == (label == "Web" and 276 or 131), case[1])
            end
        else
            local last_box_row, edge = 0, 255
            for y = 54, word.y0 - 1 do
                local gray = frame:getPixel(632, y):getColor8().a
                if gray ~= 255 then last_box_row, edge = y, gray end
            end
            if dpi == 96 then
                assert(word.y0 == (i == 14 and 164 or 54), case[1])
                assert(last_box_row == (i == 14 and 163 or 0), case[1])
                assert(edge == (i == 14 and 68 or 255), case[1])
            end
            print(string.format("FIT H %s | %s | page=%d word=%d..%d preceding=%d/%d", label, case[1], page, word.y0, word.y1, last_box_row, edge))
        end
    end
    for _, i in ipairs({1, 9, 13, 14, 16}) do
        view:gotoPage(pages[i])
        view:drawCurrentPage(frame, false, false, false, false)
        frame:writePNG(string.format("%s/fit-height-stock-%s-%d.png", output, label:lower(), i), false)
    end
    if label == "Web" then
        view:gotoPage(pages[16] - 1)
        view:drawCurrentPage(frame, false, false, false, false)
        if dpi == 96 then
            for y = 54, 160 do assert(frame:getPixel(632, y):getColor8().a == 153) end
            for y = 161, 163 do assert(frame:getPixel(632, y):getColor8().a == 68) end
            assert(frame:getPixel(632, 164):getColor8().a == 255)
        end
        frame:writePNG(output .. "/fit-height-stock-web-tail.png", false)
    end
    for next_mode = 0, 3 do
        menu:setBlockRenderingMode(next_mode)
        view:renderDocument()
        for i, anchor in ipairs(anchors) do
            assert(view:getTextFromXPointers(anchor.start, anchor["end"]) == cases[i][1])
        end
    end
    frame:free()
    view:close()
end
Device:exit()
