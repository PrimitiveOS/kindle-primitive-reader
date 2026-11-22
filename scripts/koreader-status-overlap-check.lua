-- Run from the installed KOReader directory. The stock CREngine reads an EPUB and
-- restores a private page anchor. Only numeric checks leave the isolated profile.
local dir, path = arg[1], arg[2]
assert(dir and dir:match("^/mnt/us/%.primitive%-reader%-parity%.[%w]+/[%w%-]+/[%w%-]+$"))
assert(path and path:sub(1, 18) == "/mnt/us/documents/" and not path:find("/%.%./"))
require("setupkoenv")
local lfs = require("libs/libkoreader-lfs")
assert(lfs.symlinkattributes(dir, "mode") == "directory")
assert(lfs.symlinkattributes(path, "mode") == "file")
local cre = require("libs/libkoreader-cre")
local bb = require("ffi/blitbuffer")
local width, height = 1264, 1680
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
-- Exercise the installed margin handler at a controlled 1:1 screen scale and a
-- 74-pixel footer. The configured values match Reader's viewport comparison.
local code = assert(io.open("frontend/apps/reader/modules/readertypeset.lua", "rb"))
local source = code:read("*all")
code:close()
local at = assert(source:find("function ReaderTypeset:onSetPageMargins(", 1, true))
local last = assert(source:find("\nend", at, true))
local body = source:sub(at, last+3):gsub("^function [^(]+%(", "return function(self, ", 1)
local load = assert(loadstring(body))
local env = {Screen = {scaleBySize = function(self,n) return n end}, Event = require("ui/event")}
setfenv(load, env)
local handler = load()
local actual
local state = {view = {footer = {getHeight = function() return 74 end}},
    ui = {document = {setPageMargins = function(self,l,t,r,b)
        actual = b
        view:setPageMargins(l,t,r,b)
    end}, handleEvent = function() end}}
local function overlap(enabled)
    state.view.footer.reclaim_height = enabled
    handler(state,{54,54,54,0})
    assert(actual == (enabled and 0 or 74))
end
overlap(false)
assert(view:loadDocument(path, false))
view:renderDocument()
local old_count = view:getPages()
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
local function frame()
    view:gotoPage(view:getPageFromXPointer(anchor))
    local image = bb.new(width,height,bb.TYPE_BB8)
    view:drawCurrentPage(image,false,false,false,false)
    local raster = bb.tostring(image)
    image:free()
    return raster, view:getWordBoxesFromPositions(anchor,ending,true)
end
local original = frame()
assert(#original == width * height)
local anchor_page_old = view:getPageFromXPointer(anchor)
overlap(true)
view:renderDocument()
local new_count = view:getPages()
assert(new_count <= old_count)
local expanded, boxes = frame()
assert(#expanded == #original and boxes and #boxes > 0)
local anchor_page_full = view:getPageFromXPointer(anchor)
-- The same source anchor has a page and at least one word box in both geometries.
-- Count the ink in the additional 74 rows. This aligned passage can occur anywhere
-- on the stock page, so a short final page can legitimately have an empty band.
local function ink(image)
    local dark = 0
    for i = (height-74)*width+1, height*width do
        if image:byte(i) < 245 then dark = dark+1 end
    end
    return dark
end
assert(ink(original) == 0)
overlap(false)
view:renderDocument()
assert(view:getPages() == old_count and view:getPageFromXPointer(anchor) == anchor_page_old)
assert(frame() == original)
view:close()
print(string.format('{"reserved":74,"full_height":1680,"original_pages":%d,"full_pages":%d,"passage_page_before":%d,"passage_page_after":%d,"source_boxes":%d,"additional_ink":%d,"reset_identical":true}',
    old_count, new_count, anchor_page_old, anchor_page_full, #boxes, ink(expanded)))
