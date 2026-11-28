-- Native replay of word spacing around a float (F-9 float-contour oracle) on the pinned runtime.
-- Lays out a paragraph that wraps beside a float card at 100% and 95% word scale, with the
-- default 75% condensing, and reports per-line gap changes. Lines beside the float start at
-- a larger x than the margin, so the contour lines can be told apart from the rest.
-- Run from the KOReader install directory:
--   KO_HOME="$scratch/profile" ./luajit "$reader/scripts/koreader-float-word-spacing-check.lua" \
--     "$scratch/cache" "$reader/tests/fixtures/word-spacing-float.epub" \
--     "$reader/tests/fixtures/fonts/NotoSans-Regular.ttf" "$reader/tests/fixtures/word-spacing-float.words"
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
local Device = require("device")
require("document/canvascontext"):init(Device)
local Cre = require("libs/libkoreader-cre")
require("ui/widget/notification").notify = function() end

local cache, source, font, words_file = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)
local words = {}
for w in io.lines(words_file) do words[#words + 1] = w end

local width, height = 1264, 1606
local view = Cre.newDocView(width, height, 1)
view:setStyleSheet("./data/epub.css", "")
view:setVisiblePageCount(1, false)
view:setHeaderInfo(0)
view:setPageMargins(54, 54, 54, 54)
view:setFontFace("Noto Sans")
view:setFontSize(42)
view:setIntProperty("crengine.render.dpi", 96)
view:setIntProperty("font.hinting.mode", 0)
assert(view:loadDocument(source, false))

local function position(word)
    local hit = assert((view:findAllText(word, false, false, 8, true, 0, 255) or {})[1], word)
    local page = view:getPageFromXPointer(hit.start)
    local y, x = view:getPosFromXPointer(hit.start)
    return {page = page, y = y, x = x}
end

local function measure(scale)
    view:setIntProperty("crengine.style.space.width.scale.percent", scale)
    view:setIntProperty("crengine.style.space.condensing.percent", 75)
    view:renderDocument()
    local out = {}
    for index, word in ipairs(words) do out[index] = position(word) end
    return out
end

local base = measure(100)
local scaled = measure(95)
-- Group words by (page, line y), then report each line's gap changes.
local lines, order = {}, {}
for index = 1, #words do
    local key = base[index].page .. ":" .. base[index].y
    if not lines[key] then lines[key] = {}; order[#order + 1] = key end
    table.insert(lines[key], index)
end
for _, key in ipairs(order) do
    local idx = lines[key]
    local deltas = {}
    for k = 1, #idx - 1 do
        local a, b = idx[k], idx[k + 1]
        if scaled[b].y == scaled[a].y and base[b].y == base[a].y then
            local base_gap = base[b].x - base[a].x
            local gap = scaled[b].x - scaled[a].x
            deltas[#deltas + 1] = tostring(gap - base_gap)
        end
    end
    print(string.format("FLOATLINE page=%s y=%s start_x=%d words=%d deltas=%s", key:match("^(%d+)"),
        tostring(base[idx[1]].y), base[idx[1]].x, #idx, table.concat(deltas, ",")))
end
view:close()
Device:exit()
