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
    {"Physical sides marker", 221, 94, 1092},
    {"Font and percent marker", 204, 109, 1078},
    {"Minimum inset marker", 187, 55, 1155},
}
local baseline = "Following ordinary text"
local function locate(view, phrase)
    local hit = assert((view:findAllText(phrase, false, false, 8, true, 0, 255) or {})[1])
    assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase)
    return hit
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
        local hit = locate(view, case[1])
        saved[index] = hit
        local page = view:getPageFromXPointer(hit.start)
        assert(page == 1)
        view:gotoPage(page)
        local word = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true)[1])
        view:drawCurrentPage(frame, false, false, false, false)
        local left, right = width, -1
        for x = 0, width - 1 do
            if frame:getPixel(x, word.y0 + 3):getColor8().a == case[2] then
                left, right = math.min(left, x), math.max(right, x)
            end
        end
        assert(word.x0 == case[3] and left == case[3] and right - left + 1 == case[4])
        print(string.format("MARGIN %s|%d|text=%d|x=%d|w=%d", modes[mode+1], index, word.x0, left, right-left+1))
    end
    local hit = locate(view, baseline)
    view:gotoPage(view:getPageFromXPointer(hit.start))
    view:drawCurrentPage(frame, false, false, false, false)
    frame:writePNG(captures .. "/" .. modes[mode+1] .. ".png", false)
    if mode == 3 then
        for _, next_mode in ipairs{0, 1, 2, 3} do
            typeset:setBlockRenderingMode(next_mode)
            view:renderDocument()
            for index, case in ipairs(cases) do
                local anchor = saved[index]
                assert(view:getTextFromXPointers(anchor.start, anchor["end"]) == case[1])
                view:gotoXPointer(anchor.start)
            end
        end
    end
    frame:free()
    view:close()
end
Device:exit()
