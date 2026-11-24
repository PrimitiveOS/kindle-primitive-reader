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
local function configure(view)
    view:setStyleSheet("./data/epub.css", "")
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(54, 54, 54, 54)
    view:setFontSize(40)
    view:setDefaultInterlineSpace(120)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setFontFace("Noto Sans")
end
local function attach(view)
    local updates = 0
    local typeset = setmetatable({ ui = {
        document = { setBlockRenderingFlags = function(_, flags) view:setIntProperty(property, flags) end },
        handleEvent = function(_, event) assert(event.handler == "onUpdatePos"); updates = updates + 1 end,
    }}, { __index = Typeset })
    return typeset, function() return updates end
end

local modes = {"Legacy", "Flat", "Book", "Web"}
local cases = {
    {phrase="Ashore", shade=221, side="left", outer_x=43, inner_x=63,
        delta={0, 1, 1, -19}, x={54, 54, 54, 34}, outside={34, 53}},
    {phrase="wind", shade=204, side="right", outer_x=1222, inner_x=1200,
        delta={0, 0, 0, 20}, x={66, 66, 66, 66}, outside={1210, 1229}},
}
local function find_pair(view, phrase)
    local hits = view:findAllText(phrase, false, false, 8, true, 0, 255)
    assert(#hits == 2 and view:getTextFromXPointers(hits[2].start, hits[2]["end"]) == phrase)
    return hits[1], hits[2]
end

for mode = 0, 3 do
    local view = Cre.newDocView(width, height, 1)
    local typeset = attach(view)
    typeset:setBlockRenderingMode(mode)
    configure(view)
    assert(view:loadDocument(source, false))
    view:renderDocument()
    local frame = BB.new(width, height, BB.TYPE_BB8)
    local saved = {}
    for index, case in ipairs(cases) do
        local ordinary, hit = find_pair(view, case.phrase)
        saved[index] = hit
        local page = view:getPageFromXPointer(hit.start)
        assert(page == 1)
        view:gotoPage(page)
        local word = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true)[1])
        local neutral = assert(view:getWordBoxesFromPositions(ordinary.start, ordinary["end"], true)[1])
        local delta = case.side == "left" and word.x0 - neutral.x0 or word.x1 - neutral.x1
        view:drawCurrentPage(frame, false, false, false, false)
        local left, right = width, -1
        for x = 0, width - 1 do
            if frame:getPixel(x, word.y0 + 3):getColor8().a == case.shade then
                left, right = math.min(left, x), math.max(right, x)
            end
        end
        local y = math.floor((word.y0 + word.y1) / 2)
        local href = view:getLinkFromPosition(case.outer_x, y)
        local quote = view:getTextFromPositions(case.outer_x, y, case.outer_x, y, false, false)
        local outer_ink = 0
        for row = word.y0, word.y1 - 1 do
            for x = case.outside[1], case.outside[2] do
                if frame:getPixel(x, row):getColor8().a < 112 then
                    outer_ink = outer_ink + 1
                end
            end
        end
        assert(delta == case.delta[mode+1] and left == case.x[mode+1])
        assert(right-left+1 == (mode == 3 and 1164 or 1144))
        -- CREngine's exact link hit test clips user page margins. Its text
        -- range lookup instead pulls the point into the page for selection.
        assert(href == "" and quote.text == case.phrase)
        local inner_href = view:getLinkFromPosition(case.inner_x, y)
        assert(inner_href ~= "")
        assert((outer_ink > 0) == (mode == 3))
        print(string.format("SIGNED %s|%s|delta=%d|x=%d|w=%d|outer_ink=%d|strict_href=%s|word=%s",
            modes[mode+1], case.side, delta, left, right-left+1, outer_ink, href, quote.text))
    end
    view:drawCurrentPage(frame, false, false, false, false)
    frame:writePNG(captures .. "/" .. modes[mode+1] .. ".png", false)
    if mode == 3 then
        for _, next_mode in ipairs{0, 1, 2, 3} do
            typeset:setBlockRenderingMode(next_mode)
            view:renderDocument()
            for index, case in ipairs(cases) do
                local anchor = saved[index]
                assert(view:getTextFromXPointers(anchor.start, anchor["end"]) == case.phrase)
                view:gotoXPointer(anchor.start)
            end
        end
    end
    frame:free()
    view:close()
end
Device:exit()
