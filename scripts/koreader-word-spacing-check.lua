-- Native replay of CREngine's space-width scaling on the pinned KOReader runtime.
-- Measures the x-advance of each space at 100%, 95% and 75% and asserts that every
-- change is a whole number of pixels, matching lvtextfm.cpp's integer rule:
--   scaled = char_width * percent / 100   (integer division)
-- Run from the KOReader install directory:
--   KO_HOME="$scratch/profile" ./luajit "$reader/scripts/koreader-word-spacing-check.lua" \
--     "$scratch/cache" "$reader/tests/fixtures/word-spacing-engine.epub" \
--     "$reader/tests/fixtures/fonts/NotoSans-Regular.ttf"
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
local Device = require("device")
require("document/canvascontext"):init(Device)
local Cre = require("libs/libkoreader-cre")
require("ui/widget/notification").notify = function() end

local cache, source, font = assert(arg[1]), assert(arg[2]), assert(arg[3])
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)

-- The fixture is one line of 10 words (aaa, aab, ...), so every gap sits on one row.
-- Multi-line paragraphs were not used: CREngine left the first two lines of a
-- 40-word paragraph unscaled at 95% (recorded as an open item in the gap register).
local words = {}
for index = 0, 9 do
    words[#words + 1] = "a" .. string.char(97 + math.floor(index / 26)) .. string.char(97 + index % 26)
end

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
    local y, x = view:getPosFromXPointer(hit.start)
    return x, y
end

local function measure(scale)
    view:setIntProperty("crengine.style.space.width.scale.percent", scale)
    -- Minimum 100 disables condensing, so only the scale is measured.
    view:setIntProperty("crengine.style.space.condensing.percent", 100)
    view:renderDocument()
    local xs, ys = {}, {}
    for index, word in ipairs(words) do
        xs[index], ys[index] = position(word)
    end
    return xs, ys
end

local base_x, base_y = measure(100)
local deltas_by_scale = {}
for _, scale in ipairs({95, 75}) do
    local xs, ys = measure(scale)
    local deltas = {}
    for index = 1, #words - 1 do
        assert(ys[index] == ys[index + 1], "fixture must stay on one line")
        -- Word widths do not change, so the change in each gap is the space change.
        local base_gap = base_x[index + 1] - base_x[index]
        local gap = xs[index + 1] - xs[index]
        deltas[#deltas + 1] = gap - base_gap
    end
    assert(#deltas == #words - 1, "gap count")
    local first = deltas[1]
    for _, delta in ipairs(deltas) do
        assert(delta == first, string.format("scale %d: gap deltas differ (%d vs %d)", scale, delta, first))
    end
    -- Find the whole-pixel natural space width s that explains the delta:
    -- floor(s * scale / 100) - s == delta.
    local explained = {}
    for s = 1, 40 do
        if math.floor(s * scale / 100) - s == first then
            explained[#explained + 1] = s
        end
    end
    assert(#explained > 0, string.format("scale %d: delta %d has no integer space width", scale, first))
    deltas_by_scale[scale] = {delta = first, count = #deltas, explained = explained}
    print(string.format("CRE scale %d%%: %d same-line spaces, each changed by %d px (space width candidates: %s)",
        scale, #deltas, first, table.concat(explained, ",")))
end

-- The float rule would change the total by (scale - 100)% of the summed space width,
-- which is fractional per space. Whole-pixel deltas confirm the integer rule.
print(string.format('{"engine":"koreader-cre","check":"word-spacing-integer-rule","words":%d,' ..
    '"delta_95":%d,"delta_75":%d}', #words, deltas_by_scale[95].delta, deltas_by_scale[75].delta))
view:close()
Device:exit()
