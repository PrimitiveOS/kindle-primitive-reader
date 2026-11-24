-- Replay authored text-only inline boxes with the four pinned ReaderTypeset masks.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
require("document/canvascontext"):init(require("device"))
local Typeset = require("apps/reader/modules/readertypeset")
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")

local source, cache, font, captures = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
local width, height = 600, 720
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)

local property = "crengine.render.block.rendering.flags"
local modes = {"Legacy", "Flat", "Book", "Web"}
local flags = {0x00000000, 0x03030031, 0x03375131, 0x7FFFFFFF}
local expected = {
    -- fixed / intrinsic / narrow / percentage paint, and x of their next host words.
    {91, 160, 177, 127, 216, 266, 307, 297},
    {192, 164, 104, 211, 316, 270, 233, 381},
    {216, 164, 120, 211, 340, 270, 249, 381},
    {216, 164, 120, 211, 340, 270, 249, 381},
}
local names = {
    "Trail", "Café", "opens", "Sea", "glass", "edge", "shines", "Wind", "River", "bend",
    "rests", "Lantern", "Shelter", "lights", "Across", "Distant", "water", "Arrival",
}
local function find(view, phrase)
    for _, hit in ipairs(view:findAllText(phrase, false, false, 8, true, 0, 255) or {}) do
        if view:getTextFromXPointers(hit.start, hit["end"]) == phrase then
            local page = view:getPageFromXPointer(hit.start)
            view:gotoPage(page)
            local word = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true)[1])
            return {page = page, x = word.x0, y = word.y0}
        end
    end
    error("No exact hit for " .. phrase)
end

local function band(frame, y, color)
    local left, right = width, -1
    for x = 0, width - 1 do
        if frame:getPixel(x, y):getColor8().a == color then
            left, right = math.min(left, x), math.max(right, x)
        end
    end
    return math.max(0, right - left + 1)
end

for ordinal, mode in ipairs(modes) do
    local view = Cre.newDocView(width, height, 1)
    local typeset = setmetatable({ui = {
        document = {setBlockRenderingFlags = function(_, mask) view:setIntProperty(property, mask) end},
        handleEvent = function(_, event) assert(event.handler == "onUpdatePos") end,
    }}, {__index = Typeset})
    typeset:setBlockRenderingMode(ordinal - 1)
    assert(view:getIntProperty(property) == flags[ordinal])
    view:setStyleSheet("./data/epub.css", "")
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(36, 36, 36, 36)
    view:setFontSize(32)
    view:setDefaultInterlineSpace(120)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setFontFace("Noto Sans")
    assert(view:loadDocument(source, false))
    view:renderDocument()
    local marks = {}
    for _, name in ipairs(names) do
        local pos = find(view, name)
        marks[name] = pos
        print(string.format("%s|%s|p=%d|x=%d|y=%d", mode, name, pos.page, pos.x, pos.y))
    end
    view:gotoPage(marks.Trail.page)
    local frame = BB.new(width, height, BB.TYPE_BB8)
    view:drawCurrentPage(frame, false, false, false, false)
    local dimensions = expected[ordinal]
    for i, probe in ipairs{{"Café", 210}, {"glass", 204}, {"River", 204}, {"Shelter", 222}} do
        local pos = marks[probe[1]]
        assert(pos.page == marks.Trail.page)
        local span = band(frame, pos.y + 2, probe[2])
        assert(span == dimensions[i], mode .. " gray band for " .. probe[1])
    end
    for i, follower in ipairs{"opens", "shines", "rests", "lights"} do
        assert(marks[follower].x == dimensions[i + 4], mode .. " advance before " .. follower)
    end
    if mode == "Legacy" then
        assert(marks.River.y == marks.bend.y)
    else
        assert(marks.River.y < marks.bend.y and marks.River.x == marks.bend.x)
        assert(marks.Across.y < marks.Distant.y)
    end
    if mode ~= "Book" then
        frame:writePNG(captures .. "/render-inline-blocks-stock-" .. mode:lower() .. ".png", false)
    end
    print("Inline-block geometry passed in " .. mode)
    frame:free()
    view:close()
end
