-- Pixel check for wrapped paragraphs on the pinned KOReader runtime.
-- Draws the page and reports the x-start of each ink run per text line at 100% and 95% word
-- scale. Finding: lines that wrap to a next line keep natural spaces; the last line of a
-- paragraph (and a single-line paragraph) shrinks. Fixture: tests/fixtures/word-spacing-wrapped.epub.
-- Run: ./luajit "$reader/scripts/koreader-word-spacing-lines-check.lua" "$scratch/cache" "$reader/tests/fixtures/word-spacing-wrapped.epub" "$reader/tests/fixtures/fonts/NotoSans-Regular.ttf"
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
local Device = require("device")
require("document/canvascontext"):init(Device)
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
require("ui/widget/notification").notify = function() end
local cache, source, font = arg[1], arg[2], arg[3]
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)
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
local frame = BB.new(width, height, BB.TYPE_BB8)
local function ink(x, y) return frame:getPixelP(x, y)[0].a < 160 end
local function measure(scale)
    view:setIntProperty("crengine.style.space.width.scale.percent", scale)
    view:setIntProperty("crengine.style.space.condensing.percent", 75)
    view:renderDocument()
    view:gotoPage(1)
    frame:fill(BB.COLOR_WHITE)
    view:drawCurrentPage(frame, false, false, false, false)
    -- Row bands of ink.
    local rows, bands = {}, {}
    for y = 0, height - 1 do
        local any = false
        for x = 0, width - 1, 2 do if ink(x, y) then any = true break end end
        rows[y] = any
    end
    local y = 0
    while y < height do
        if rows[y] then
            local top = y
            while y < height and rows[y] do y = y + 1 end
            bands[#bands + 1] = {top, y - 1}
        end
        y = y + 1
    end
    local report = {}
    for index, band in ipairs(bands) do
        -- Column ink runs inside the band; gaps wider than 6 px separate words.
        local cols = {}
        for x = 0, width - 1 do
            local any = false
            for yy = band[1], band[2] do if ink(x, yy) then any = true break end end
            cols[x] = any
        end
        local starts, last, gap_list = {}, nil, {}
        local x = 0
        while x < width do
            if cols[x] then
                local s = x
                while x < width and cols[x] do x = x + 1 end
                if last and s - last > 6 then starts[#starts + 1] = s end
                if last == nil then starts[#starts + 1] = s end
                last = x - 1
            else
                x = x + 1
            end
        end
        report[#report + 1] = string.format("band %d y=%d-%d words=%d starts=%s", index, band[1], band[2], #starts, table.concat(starts, ","))
    end
    return report
end
local r100 = measure(100)
local r95 = measure(95)
for _, line in ipairs(r100) do print("S100 " .. line) end
for _, line in ipairs(r95) do print("S95 " .. line) end
frame:free()
view:close()
Device:exit()
