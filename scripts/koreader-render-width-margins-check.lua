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
local signed = arg[5] == "signed"
local fit = arg[5] == "fit-content"
local cases = {
    {"Fixed marker", 221, {94, 1092}, {94, 320}},
    {"Trailing marker", 204, {54, 1100}, {692, 462}},
    {"Maximum marker", 187, {54, 1156}, {442, 380}},
    {"Minimum marker", 170, {54, 1156}, {424, 416}},
    {"Auto cap marker", 153, {54, 1156}, {422, 420}},
    {"Overflow marker", 136, {134, 956}, {134, 1000}},
    {"Direction marker", 119, {94, 1092}, {890, 320}},
    {"Natural marker", 102, {106, 1092}, {106, 1092}},
}
if signed then
    cases = {
        {"Left beacon", 221, {54, 1144}, {30, 400}},
        {"Right beacon", 204, {54, 1156}, {754, 480}, "right"},
        {"Reef line", 187, {54, 1156}, {54, 1210}, "right", 1354},
        {"Low headland", 170, {54, 1116}, {0, 1220}, "left", -20},
        {"Turning basin", 153, {94, 1116}, {110, 1100}, "right"},
        {"One small lookout", 136, {54, 1156}, {22, 420}},
        {"Saltbush", 119, {54, 1148}, {20, 1244}, "right", 1314},
        {"Ebb gate", 102, {54, 1156}, {92, 1172}, "right", 1294},
    }
end
if fit then
    cases = {
        {"Shore marker", 221, {54, 1156}, {54, 258}, {54, 258}},
        {"Harbor marker", 211, {54, 1156}, {491, 282}, {491, 282}},
        {"Anchor marker", 201, {54, 1100}, {871, 283}, {871, 283}},
        {"Beacon marker", 191, {54, 1156}, {557, 150}, {557, 150}},
        {"Needle marker", 181, {54, 1156}, {422, 420}, {422, 420}},
        {"Sounding marker", 141, {54, 1144}, {30, 328}, {54, 328}},
        {"Current marker", 131, {74, 1124}, {74, 1124}, {74, 1124}},
        {"Ledger marker", 121, {54, 1156}, {459, 346}, {491, 281}},
        {"Quartz marker", 111, {54, 1156}, {30, 1234}, {54, 275}},
        {"Turn marker", 81, {94, 1092}, {974, 236}, {974, 236}},
    }
end
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
    for i, case in ipairs(cases) do
        local hit = locate(view, case[1])
        saved[i] = hit
        local page = view:getPageFromXPointer(hit.start)
        assert(page == 1)
        view:gotoPage(page)
        local word = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true)[1])
        view:drawCurrentPage(frame, false, false, false, false)
        local left, widest = 0, 0
        for y = word.y0, word.y0 + 4 do
            local run = 0
            for x = 0, width - 1 do
                run = frame:getPixel(x, y):getColor8().a == case[2] and run + 1 or 0
                if run > widest then left, widest = x + 1 - run, run end
            end
        end
        local expected = case[mode == 3 and 4 or fit and mode == 2 and 5 or 3]
        print(string.format("WIDTH %s|%d|word=%d..%d|x=%d|w=%d", modes[mode+1],
            i, word.x0, word.x1, left, widest))
        assert(left == expected[1] and widest == expected[2])
        local right_aligned = case[5] == "right" or (fit and i == 10) or (not signed and not fit and i == 7)
        local edge = (mode == 3 and case[6]) or (right_aligned and left + widest or left)
        assert(math.abs((right_aligned and word.x1 or word.x0) - edge) <= (right_aligned and 1 or 0))
        if i == (fit and 6 or 1) then
            local x = math.max(word.x0 + 8, 60)
            assert(view:getLinkFromPosition(x, math.floor((word.y0 + word.y1) / 2)) ~= "")
        end
    end
    view:drawCurrentPage(frame, false, false, false, false)
    frame:writePNG(captures .. "/" .. modes[mode+1] .. ".png", false)
    if mode == 3 then
        for _, next_mode in ipairs{0, 1, 2, 3} do
            typeset:setBlockRenderingMode(next_mode)
            view:renderDocument()
            for i, case in ipairs(cases) do
                local anchor = saved[i]
                assert(view:getTextFromXPointers(anchor.start, anchor["end"]) == case[1])
                view:gotoXPointer(anchor.start)
            end
        end
    end
    frame:free()
    view:close()
end
Device:exit()
