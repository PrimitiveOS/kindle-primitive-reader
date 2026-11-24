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
local cases = {"Bare", "Padding", "Margin", "Border", "Height", "Background", "Decorated", "Removed",
               "Nested", "OnlyBefore", "OnlyAfter", "Mixed", "Between", "One", "Text", "BareOne",
               "BareThree", "BareOnlyBefore", "BareOnlyAfter", "BareMixed", "BareBetween", "NestedBare",
               "BareThenPadding", "PaddingThenBare", "PaddingAcrossBare"}
local compact = {1,1,1,1,1,1,1,0,1,1,1,1,1,1,2,1,1,1,1,1,1,1,1,1,1}
local advances = {
    Legacy = {3,3,3,3,3,3,3,0,4,2,2,2,2,2,2,2,4,2,2,2,1,4,3,3,1},
    Flat = compact, Book = compact,
    Web = {2,3,2,3,3,2,3,0,4,2,2,2,2,2,2,1,3,2,2,2,2,2,3,2,1},
}
for mode, name in ipairs{"Legacy", "Flat", "Book", "Web"} do
    local view = Cre.newDocView(width, height, 1)
    local typeset = setmetatable({ ui = {
        document = { setBlockRenderingFlags = function(_, flags)
            view:setIntProperty("crengine.render.block.rendering.flags", flags)
        end },
        handleEvent = function() end,
    }}, { __index = Typeset })
    typeset:setBlockRenderingMode(mode - 1)
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
    for index, case in ipairs(cases) do
        local marks = {}
        for _, side in ipairs{"first", "last"} do
            local phrase = "Test " .. case .. " " .. side .. " marker"
            local hits = assert(view:findAllText(phrase, false, false, 8, true, 0, 255))
            assert(#hits == 1, phrase)
            local hit = hits[1]
            assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase)
            local page = view:getPageFromXPointer(hit.start)
            view:gotoPage(page)
            local box = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true)[1])
            local y = view:getPosFromXPointer(hit.start)
            marks[#marks + 1] = {page = page, screen_y = box.y0, doc_y = y}
        end
        local a, b = unpack(marks)
        assert(b.page-a.page == advances[name][index], name .. " " .. case)
        print(string.format("BREAK %s|%s|pages=%d..%d|advance=%d|screen_y=%d,%d|pitch=%d",
            name, case, a.page, b.page, b.page-a.page, a.screen_y,b.screen_y,b.doc_y-a.doc_y))
        if case == "Decorated" then
            for page = a.page, b.page do
                view:gotoPage(page)
                local frame = BB.new(width, height, BB.TYPE_BB8)
                view:drawCurrentPage(frame, false, false, false, false)
                frame:writePNG(captures .. "/" .. name .. "-" .. (page-a.page) .. ".png", false)
                frame:free()
            end
        end
    end
    view:close()
end
