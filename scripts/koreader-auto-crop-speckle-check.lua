-- Measure KOReader's automatic crop box against speckle size (C-1, crop path).
-- Run from the KOReader install directory with a PDF path:
--   KO_HOME="$scratch/profile" ./luajit "$reader/scripts/koreader-auto-crop-speckle-check.lua" "$reader/tests/fixtures/speckled-scan.pdf"
require("setupkoenv")
local mupdf = require("ffi/mupdf")
local KOPTContext = require("ffi/koptcontext")
local DrawContext = require("ffi/drawcontext")
local pdf = arg[1]
for _, defect in ipairs({0.0, 1.0, 3.0, 5.0}) do
    local kc = KOPTContext.new()
    kc:setTrim(true)
    kc:setWrap(false)
    kc:setWhiteThreshold(255)
    kc:setDeviceDim(1264, 1606)
    kc:setDeviceDPI(160)
    kc:setDefectSize(defect)
    kc:setZoom(1.0)
    kc:setMargin(0.0)
    local document = mupdf.openDocument(pdf)
    local page = document:openPage(1)
    local page_w, page_h = page:getSize(DrawContext.new())
    kc:setBBox(0, 0, page_w, page_h)
    page:getPagePix(kc, 0, false)
    page:close()
    local x0, y0, x1, y1 = kc:getAutoBBox()
    print(string.format('{"check":"auto-crop-speckle","defect_size":%.1f,"bbox":[%s,%s,%s,%s]}', defect, tostring(x0), tostring(y0), tostring(x1), tostring(y1)))
    kc:free()
end
