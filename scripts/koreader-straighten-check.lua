-- Native replay of KOReader's auto-straighten on the pinned runtime (C-3 oracle).
-- For each straighten limit, optimize page 1 of a tilted scan through KOPTContext and
-- measure the residual skew of the output with a projection-profile sweep.
-- Run from the KOReader install directory:
--   KO_HOME="$scratch/profile" ./luajit "$reader/scripts/koreader-straighten-check.lua" \
--     "$scratch/out" "$reader/tests/fixtures/tilted-scan.pdf"
-- Output: one JSON line per limit with the residual angle in degrees.
local out_dir, pdf = assert(arg[1]), assert(arg[2])
os.setlocale("C", "numeric")
require("setupkoenv")
local mupdf = require("ffi/mupdf")
local KOPTContext = require("ffi/koptcontext")
local DrawContext = require("ffi/drawcontext")

-- Residual skew: the angle whose row-projection of dark pixels has the highest variance.
local function residual_skew(bb, step, limit)
    local width, height = bb:getWidth(), bb:getHeight()
    local dark = {}
    for y = 0, height - 1, step do
        for x = 0, width - 1, step do
            if bb:getPixelP(x, y)[0].a < 128 then
                dark[#dark + 1] = x
                dark[#dark + 1] = y
            end
        end
    end
    local cx, cy = width / 2, height / 2
    local best_angle, best_score = 0, -1
    local bins = math.ceil(math.sqrt(width * width + height * height) / step) + 2
    for tenth = -limit * 10, limit * 10, 2 do
        local angle = math.rad(tenth / 10)
        local cos, sin = math.cos(angle), math.sin(angle)
        local counts = {}
        for i = 1, bins do counts[i] = 0 end
        for i = 1, #dark, 2 do
            local x, y = dark[i] - cx, dark[i + 1] - cy
            local row = math.floor((y * cos - x * sin) / step) + math.floor(bins / 2)
            if row >= 1 and row <= bins then counts[row] = counts[row] + 1 end
        end
        local sum, sq = 0, 0
        for i = 1, bins do sum = sum + counts[i]; sq = sq + counts[i] * counts[i] end
        local mean = sum / bins
        local score = sq / bins - mean * mean
        if score > best_score then best_score, best_angle = score, tenth / 10 end
    end
    return best_angle
end

local limits = {0, 5, 10, 15, 25}
for _, limit in ipairs(limits) do
    local kc = KOPTContext.new()
    kc:setTrim(false)
    kc:setWrap(false)
    kc:setWhiteThreshold(255)
    kc:setIndent(false)
    kc:setColumns(1)
    kc:setDeviceDim(1264, 1606)
    kc:setDeviceDPI(160)
    kc:setStraighten(limit)
    kc:setJustification(-1)
    kc:setWritingDirection(0)
    kc:setZoom(1.0)
    kc:setMargin(0.0)
    kc:setQuality(1.0)
    kc:setContrast(1.0)
    kc:setDefectSize(1.0)
    kc:setLineSpacing(1.0)
    kc:setWordSpacing(1.0)
    local document = mupdf.openDocument(pdf)
    local page = document:openPage(1)
    -- A full-page box, as KOReader sets it from the native page size.
    local page_w, page_h = page:getSize(DrawContext.new())
    kc:setBBox(0, 0, page_w, page_h)
    page:getPagePix(kc, 0, false)
    page:close()
    kc:optimizePage()
    local bb = kc:dstToBlitBuffer()
    local path = string.format("%s/straighten-%d.png", out_dir, limit)
    bb:writePNG(path)
    print(string.format('{"check":"auto-straighten","limit_degrees":%d,"residual_degrees":%.2f,"output":"%s"}',
        limit, residual_skew(bb, 4, 15), path))
    bb:free()
    kc:free()
end
