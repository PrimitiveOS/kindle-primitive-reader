-- Run from the installed KOReader directory. The stock CREngine reads an EPUB and
-- restores a private page anchor. Only numeric checks leave the isolated profile.
local dir, path = arg[1], arg[2]
assert(dir and dir:match("^/mnt/us/%.primitive%-reader%-parity%.[%w]+/[%w%-]+/[%w%-]+$"))
assert(path and path:sub(1, 18) == "/mnt/us/documents/" and not path:find("/%.%./"))
require("setupkoenv")
local lfs = require("libs/libkoreader-lfs")
assert(lfs.symlinkattributes(dir, "mode") == "directory")
assert(lfs.symlinkattributes(path, "mode") == "file")
-- Exercise the installed per-document controls independently of the renderer.
local code = assert(io.open("frontend/apps/reader/modules/readertypeset.lua", "rb"))
local source = code:read("*all")
code:close()
local state = {unscaled_margins = {54,54,54,54}, configurable = {}, ui = {handleEvent = function() end}}
local env = {Math = require("optmath"), Event = require("ui/event")}
local function apply(name, value)
    local first = assert(source:find("function ReaderTypeset:" .. name .. "(", 1, true))
    local last = assert(source:find("\nend", first, true))
    local method = source:sub(first,last+3):gsub("^function [^(]+%(", "return function(self, ", 1)
    local load = assert(loadstring(method))
    setfenv(load,env)
    load()(state,value)
    return table.concat(state.unscaled_margins,",")
end
assert(apply("onSetPageHorizMargins",{66,42}) == "66,54,42,54")
assert(apply("onSetPageTopMargin",55) == "66,55,42,54")
assert(apply("onSetPageBottomMargin",66) == "66,55,42,66")
assert(apply("onSyncPageTopBottomMargins",true) == "66,61,42,61")
assert(apply("onSetPageBottomMargin",73) == "66,73,42,73")
assert(apply("onSyncPageTopBottomMargins",false) == "66,73,42,73")
assert(apply("onSetPageBottomMargin",72) == "66,73,42,72")

local cre = require("libs/libkoreader-cre")
local bb = require("ffi/blitbuffer")
local width, height = 1264, 1606
local cache = dir .. "/cre-cache"
if not lfs.attributes(cache) then assert(lfs.mkdir(cache)) end
cre.initCache(cache, 0, true, 1)
cre.registerFont("/mnt/us/launcher/fonts/NotoSans-Regular.ttf")
local view = cre.newDocView(width, height, 1)
view:setStyleSheet("./data/epub.css")
view:setFontFace("Noto Sans")
view:setFontSize(42)
view:setDefaultInterlineSpace(135)
view:setVisiblePageCount(1, false)
view:setHeaderInfo(0)
view:setPageMargins(54, 54, 54, 54)
assert(view:loadDocument(path, false))
view:renderDocument()
local count = view:getPages()
local anchor, ending
local file = assert(io.open(dir .. "/passages", "rb"))
for text in file:lines() do
    local hits = view:findAllText(text, false, false, 2, true, 0, 255) or {}
    if #hits == 1 and hits[1].start and hits[1]["end"] then
        anchor, ending = hits[1].start, hits[1]["end"]
        break
    end
end
file:close()
assert(anchor, "No unique passage could be aligned")
local page = view:getPageFromXPointer(anchor)
local function frame()
    view:gotoPage(view:getPageFromXPointer(anchor))
    local image = bb.new(width,height,bb.TYPE_BB8)
    view:drawCurrentPage(image,false,false,false,false)
    local raster = bb.tostring(image)
    image:free()
    return raster, view:getWordBoxesFromPositions(anchor,ending,true)
end
local original, a = frame()
assert(a and #a > 0 and #original == width*height)
view:setPageMargins(66, 54, 42, 54)
view:renderDocument()
assert(view:getPages() == count and view:getPageFromXPointer(anchor) == page)
local shifted, b = frame()
assert(#a == #b)
for i, before in ipairs(a) do
    local after = b[i]
    assert(before.x0 + 12 == after.x0 and before.x1 + 12 == after.x1)
    assert(before.y0 == after.y0 and before.y1 == after.y1)
end
local rows = 0
for y = 0, height - 1 do
    local x = y*width
    if original:sub(x+1,x+width-12) == shifted:sub(x+13,x+width) then
        rows = rows + 1
    end
end
view:setPageMargins(54,54,54,54)
view:renderDocument()
assert(frame() == original)
view:close()
print(string.format('{"provider_pages":%d,"matched_page":%d,"translated_rows":%d,"height":%d,"shift_px":12,"stock_control_transitions":7,"source_rects":%d,"source_restored":true,"reset_identical":true}',
    count,page,rows,height,#a))
