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
local width, height = 1264, 720
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
local passages = {
    "Before short group", "Short opening", "Short ending", "After short group",
    "Before oversized group", "Long opening", "Long middle", "Long ending", "After oversized group",
}
local measured = {
    {1, 438}, {2, 54}, {2, 182}, {2, 246},
    {3, 246}, {3, 310}, {4, 54}, {4, 438}, {4, 502},
}
local legacy = { {1, 438}, {1, 502}, {2, 54}, {2, 118} }
for mode = 0, 3 do
    local view = Cre.newDocView(width, height, 1)
    local typeset = attach(view)
    typeset:setBlockRenderingMode(mode)
    configure(view)
    assert(view:loadDocument(source, false))
    view:renderDocument()
    assert(view:getPages() == 4)
    local frame = BB.new(width, height, BB.TYPE_BB8)
    local anchors = {}
    for index, phrase in ipairs(passages) do
        local hits = assert(view:findAllText(phrase, false, false, 8, true, 0, 255))
        local hit = assert(hits[1])
        assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase)
        local page = view:getPageFromXPointer(hit.start)
        view:gotoPage(page)
        local mark = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true)[1])
        local expected = mode == 0 and legacy[index] or nil
        expected = expected or measured[index]
        assert(page == expected[1] and mark.y0 == expected[2])
        anchors[phrase] = hit
        print(string.format("MARK %s|%s|p=%d|y=%d", modes[mode+1], phrase, page, mark.y0))
        if mode == 3 and (phrase == "Long opening" or phrase == "Short opening") then
            view:drawCurrentPage(frame, false, false, false, false)
            frame:writePNG(captures .. "/" .. phrase:gsub(" ", "-") .. ".png", false)
        end
    end
    if mode == 3 then
        for _, next_mode in ipairs{0, 1, 2, 3} do
            typeset:setBlockRenderingMode(next_mode)
            view:renderDocument()
            for _, phrase in ipairs{"Short opening", "Long opening", "Long ending"} do
                local hit = anchors[phrase]
                assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase)
                view:gotoXPointer(hit.start)
            end
        end
    end
    frame:free()
    view:close()
end
Device:exit()
