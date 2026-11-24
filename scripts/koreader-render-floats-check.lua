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
local width, height = 1264, 1606
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)
local passages = {
    {"Left card", "Left neighbor", "Left clearance", "Left ordinary"},
    {"Right card", "Right neighbor", "Right clearance", "Right ordinary"},
    {"Opposed left", "Opposed right", "Opposing neighbor", "Left cleared only", "Both sides cleared"},
    {"Wrap card", "Wrap begins here", "Wrap ending marker", "Wrap next paragraph"},
}
local expected = {
    Book = {
        {54,54,470,54,54,118,54,182}, {794,54,54,54,54,118,54,182},
        {54,54,794,54,470,54,54,182,54,246}, {54,54,470,54,255,374,53,438},
    },
    Web = {
        {54,54,470,54,54,134,54,198}, {794,54,54,54,54,134,54,198},
        {54,54,794,54,470,54,54,214,54,278}, {54,54,470,54,713,374,53,438},
    },
}
local heights = { Book = {{64}, {64}, {64,64}, {64}}, Web = {{80}, {80}, {160,200}, {80}} }
for _, mode in ipairs{{2, "Book"}, {3, "Web"}} do
    local view = Cre.newDocView(width, height, 1)
    local typeset = setmetatable({ ui = {
        document = { setBlockRenderingFlags = function(_, flags)
            view:setIntProperty("crengine.render.block.rendering.flags", flags)
        end },
        handleEvent = function() end,
    }}, { __index = Typeset })
    typeset:setBlockRenderingMode(mode[1])
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
    for index, group in ipairs(passages) do
        local page
        for mark, phrase in ipairs(group) do
            local hits = assert(view:findAllText(phrase, false, false, 8, true, 0, 255))
            assert(#hits == 1, phrase)
            local hit = hits[1]
            assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase)
            local marked_page = view:getPageFromXPointer(hit.start)
            page = page or marked_page
            assert(page == marked_page, phrase)
            view:gotoPage(page)
            local box = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true)[1])
            local coords = expected[mode[2]][index]
            assert(box.x0 == coords[2*mark-1] and box.y0 == coords[2*mark], phrase)
            local y, x = view:getPosFromXPointer(hit.start)
            print(string.format("MARK %s|%s|p=%d|x=%d|y=%d|doc_x=%d|doc_y=%d",
                mode[2],phrase,page,box.x0,box.y0,x,y))
        end
        local frame = BB.new(width, height, BB.TYPE_BB8)
        view:drawCurrentPage(frame, false, false, false, false)
        local bands = {}
        for _, edge in ipairs{{454, 204}, {1200, 221}} do
            local top, bottom = 54, 54
            while bottom < height and frame:getPixel(edge[1], bottom):getColor8().a == edge[2] do
                bottom = bottom + 1
            end
            if bottom > top then
                bands[#bands+1] = bottom - top
                print(string.format("BAND %s|%d|tone=%d|top=%d|height=%d",mode[2],index,edge[2],top,bottom-top))
            end
        end
        assert(#bands == #heights[mode[2]][index])
        for band, value in ipairs(bands) do assert(value == heights[mode[2]][index][band]) end
        frame:writePNG(captures .. "/" .. mode[2] .. "-" .. index .. ".png", false)
        frame:free()
    end
    view:close()
end
