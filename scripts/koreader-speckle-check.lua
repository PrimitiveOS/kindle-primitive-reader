-- Native replay of KOReader's reflow speckle ignore size on the pinned runtime (C-1 oracle).
-- Reflows a scanned page through KOPTContext with text wrap on, for each defect size
-- (KOReader's stock values 1.0, 3.0, 5.0), and counts the isolated dark pixels that
-- survive in the output. Isolated means no dark pixel among its eight neighbours.
-- Run from the KOReader install directory:
--   KO_HOME="$scratch/profile" ./luajit "$reader/scripts/koreader-speckle-check.lua" \
--     "$scratch/out" "$reader/tests/fixtures/speckled-scan.pdf"
local out_dir, pdf = assert(arg[1]), assert(arg[2])
os.setlocale("C", "numeric")
require("setupkoenv")
local mupdf = require("ffi/mupdf")
local KOPTContext = require("ffi/koptcontext")
local DrawContext = require("ffi/drawcontext")

local function isolated_dark(bb)
    local width, height = bb:getWidth(), bb:getHeight()
    local dark = {}
    for y = 0, height - 1 do
        for x = 0, width - 1 do
            dark[y * width + x] = bb:getPixelP(x, y)[0].a < 128
        end
    end
    local count, total = 0, 0
    for y = 1, height - 2 do
        for x = 1, width - 2 do
            if dark[y * width + x] then
                total = total + 1
                local neighbours = false
                for dy = -1, 1 do
                    for dx = -1, 1 do
                        if (dx ~= 0 or dy ~= 0) and dark[(y + dy) * width + (x + dx)] then
                            neighbours = true
                        end
                    end
                end
                if not neighbours then count = count + 1 end
            end
        end
    end
    return count, total
end

for _, defect in ipairs({1.0, 3.0, 5.0}) do
    local kc = KOPTContext.new()
    kc:setTrim(false)
    kc:setWrap(true)
    kc:setWhiteThreshold(255)
    kc:setIndent(true)
    kc:setColumns(1)
    kc:setDeviceDim(1264, 1606)
    kc:setDeviceDPI(160)
    kc:setStraighten(0)
    kc:setJustification(-1)
    kc:setWritingDirection(0)
    kc:setZoom(1.0)
    kc:setMargin(0.0)
    kc:setQuality(1.0)
    kc:setContrast(1.0)
    kc:setDefectSize(defect)
    kc:setLineSpacing(1.0)
    kc:setWordSpacing(1.0)
    local document = mupdf.openDocument(pdf)
    local page = document:openPage(1)
    local page_w, page_h = page:getSize(DrawContext.new())
    kc:setBBox(0, 0, page_w, page_h)
    page:getPagePix(kc, 0, false)
    page:close()
    kc:optimizePage()
    local bb = kc:dstToBlitBuffer()
    local path = string.format("%s/speckle-%s.png", out_dir, tostring(defect))
    bb:writePNG(path)
    local isolated, dark = isolated_dark(bb)
    print(string.format('{"check":"reflow-speckle","defect_size":%.1f,"isolated_dark_pixels":%d,"dark_pixels":%d,"output":"%s"}',
        defect, isolated, dark, path))
    bb:free()
    kc:free()
end
