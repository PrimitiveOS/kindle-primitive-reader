-- Compare the pinned native KOReader's fitted paragraph edges and page cuts.
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
    {"Base marker", 221}, {"Upper ledge", 204, 51, 85}, {"Lower ledge", 170},
    {"Long measure", 153}, {"Sloping bank", 136},
    {"Sounding 01", 187, 68, 68}, {"Sounding 20", 187, 68, 68}, {"Sounding 30", 187, 68, 68},
    {"Sounding 42", 187, 68, 68}, {"After the passage", 255},
}
local expected_pages = {1,1,1,1,1,2,2,3,3,4}
local expected_vertical = {
    {54,118,54,118,221,221}, {118,234,147,211,51,85}, {234,355,268,332,170,170},
    {355,511,380,461,153,153}, {511,609,534,598,136,136},
    {54,1493,85,149,68,187}, {54,1493,1301,1365,68,187},
    {54,1374,502,566,187,68}, {54,1374,1270,1334,187,68},
}
local expected_horizontal = {
    {{54,1210},{54,1210},{54,1210},{54,1210},{54,1192},{57,1207}},
    {{54,1210},{54,1210},{54,1210},{54,1210},{54,1192},{57,1207}},
    {{477,786},{478,785},{480,784},{427,837},{54,370},{470,793}},
    {{477,786},{478,785},{480,784},{427,837},{40,356},{470,793}},
}

local function locate(view, phrase)
    local hit = assert((view:findAllText(phrase, false, false, 8, true, 0, 255) or {})[1], phrase)
    assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase)
    return hit
end

local function measurement(frame, word, gray, leading, trailing)
    local best_left, best_right = 0, 0
    for y = word.y0, word.y0 + 4 do
        local run = 0
        for x = 0, width - 1 do
            run = frame:getPixel(x, y):getColor8().a == gray and run + 1 or 0
            if run > best_right - best_left then best_left, best_right = x + 1 - run, x + 1 end
        end
    end
    local x = best_right - 8
    local top, bottom = word.y0, word.y0
    local tones = {[gray]=true}
    if leading then tones[leading] = true end
    if trailing then tones[trailing] = true end
    local function inside(y)
        return tones[frame:getPixel(x, y):getColor8().a]
    end
    while top > 0 and inside(top - 1) do top = top - 1 end
    while bottom < height and inside(bottom) do bottom = bottom + 1 end
    return best_left, best_right, top, bottom, frame:getPixel(x, top):getColor8().a,
        frame:getPixel(x, bottom - 1):getColor8().a
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
    local anchors, pages = {}, {}
    for i, case in ipairs(cases) do
        local anchor = locate(view, case[1])
        anchors[i] = anchor
        local page = view:getPageFromXPointer(anchor.start)
        pages[i] = page
        assert(page == expected_pages[i], case[1])
        view:gotoPage(page)
        local word = assert(view:getWordBoxesFromPositions(anchor.start, anchor["end"], true)[1])
        if case[2] ~= 255 then
            view:drawCurrentPage(frame, false, false, false, false)
            local left, right, top, bottom, lead, tail = measurement(frame, word, case[2], case[3], case[4])
            print(string.format("FIT V %s | %s | page=%d x=%d..%d y=%d..%d word=%d..%d ends=%d/%d", label, case[1], page, left, right, top, bottom, word.y0, word.y1, lead, tail))
            local x = expected_horizontal[mode][math.min(i, 6)]
            local y = expected_vertical[i]
            local word_delta = mode == 1 and i == 5 and -1 or 0
            assert(left == x[1] and right == x[2], case[1])
            assert(top == y[1] and bottom == y[2] and word.y0 == y[3] + word_delta
                and word.y1 == y[4] + word_delta and lead == y[5] and tail == y[6], case[1])
            if i == 2 then
                local text_x = word.x0 + 12
                assert(view:getLinkFromPosition(text_x, word.y0 + 10) ~= "")
                local nearby = view:getLinkFromPosition(text_x, top + 10)
                print(string.format("FIT V NEAR LINK | %s | upper padding resolves=%s", label, tostring(nearby ~= "")))
            end
        else
            assert(word.y0 == 54 and word.y1 == 118, case[1])
            print(string.format("FIT V %s | %s | page=%d word=%d..%d", label, case[1], page, word.y0, word.y1))
        end
    end
    for _, i in ipairs({1, 6, 9}) do
        view:gotoPage(pages[i])
        view:drawCurrentPage(frame, false, false, false, false)
        frame:writePNG(string.format("%s/fit-vertical-stock-%s-%d.png", output, label:lower(), i), false)
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
