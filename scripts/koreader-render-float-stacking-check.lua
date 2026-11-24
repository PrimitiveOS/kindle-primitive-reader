require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
require("document/canvascontext"):init(require("device"))
local Typeset = require("apps/reader/modules/readertypeset")
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")

local source, cache, font, captures = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
local width, height = 1264, 1606
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)

local groups = {
    {"Long edge", "Inner edge", "Opposite bank", "Upper shelf", "Middle shelf", "Lower shelf", "Open bank", "Clear bank"},
    {"Outer quay", "Inner quay", "Below the crane", "Clear quay"},
    {"Outer card", "Right wall", "Lower card", "Small card", "Water edge", "Clear water"},
    {"First beam", "Next beam", "Along beam", "After beams"},
    {"Far step", "High ledge", "Mid ledge", "Low ledge", "Clear ledge"},
}
local expected = {
    Legacy = {
        {54,54, 54,214, 54,294, 54,534, 54,598, 54,662, 54,726, 54,790},
        {54,54, 54,294, 54,374, 54,438},
        {54,54, 54,150, 54,390, 54,550, 54,670, 54,734},
        {54,54, 54,214, 54,294, 54,358},
        {54,854, 54,934, 54,998, 54,1062, 54,1126},
    },
    Flat = {
        {54,54, 54,118, 54,182, 54,246, 54,310, 54,374, 54,438, 54,502},
        {54,54, 54,118, 54,182, 54,246},
        {54,54, 54,118, 54,182, 54,246, 53,310, 54,374},
        {54,54, 54,118, 53,182, 53,246},
        {54,374, 54,438, 54,502, 54,566, 54,630},
    },
    Book = {
        {54,54, 254,54, 970,54, 554,54, 54,118, 54,182, 54,246, 54,310},
        {970,54, 670,54, 54,54, 54,118},
        {54,54, 970,54, 54,118, 1050,118, 734,54, 54,246},
        {54,54, 54,118, 314,54, 53,182},
        {804,54, 954,54, 54,118, 54,182, 54,246},
    },
    Web = {
        {54,54, 254,54, 970,54, 554,54, 554,118, 254,182, 54,246, 54,310},
        {970,54, 670,54, 54,54, 54,294},
        {54,54, 970,54, 54,150, 810,150, 734,54, 54,310},
        {54,54, 54,214, 254,54, 53,294},
        {804,54, 954,54, 954,118, 804,182, 54,246},
    },
}
local capture_names = { [1] = "shelf", [3] = "dropped", [5] = "dense" }
local masks = {0x00000000, 0x03030031, 0x03375131, 0x7FFFFFFF}
for ordinal, mode in ipairs{"Legacy", "Flat", "Book", "Web"} do
    local view = Cre.newDocView(width, height, 1)
    local mask
    local typeset = setmetatable({ ui = {
        document = { setBlockRenderingFlags = function(_, flags)
            mask = flags
            view:setIntProperty("crengine.render.block.rendering.flags", flags)
        end },
        handleEvent = function(_, event) assert(event.handler == "onUpdatePos") end,
    }}, { __index = Typeset })
    typeset:setBlockRenderingMode(ordinal - 1)
    assert(mask == masks[ordinal])
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
    for index, group in ipairs(groups) do
        local page
        for i, phrase in ipairs(group) do
            local hits = assert(view:findAllText(phrase, false, false, 8, true, 0, 255))
            assert(#hits == 1, phrase)
            local hit = hits[1]
            assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase)
            local marked = view:getPageFromXPointer(hit.start)
            page = page or marked
            assert(page == marked, phrase)
            view:gotoPage(page)
            local box = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true)[1])
            local coords = expected[mode][index]
            assert(box.x0 == coords[2*i-1] and box.y0 == coords[2*i], mode .. " " .. phrase)
        end
        local capture = capture_names[index]
        if (mode == "Book" or mode == "Web") and capture then
            local frame = BB.new(width, height, BB.TYPE_BB8)
            view:drawCurrentPage(frame, false, false, false, false)
            frame:writePNG(captures .. "/render-float-stacking-stock-" .. mode:lower() .. "-" .. capture .. ".png", false)
            frame:free()
        end
    end
    print("Float stacking and exact source marks passed in " .. mode)
    view:close()
end
