-- Run in the installed KOReader directory with a one-based physical PDF page.
-- Read MuPDF's native dimensions and exercise the installed zoom handler.
local path, n = arg[1], tonumber(arg[2])
assert(path and path:sub(1,18)=="/mnt/us/documents/" and not path:find("/%.%./") and n and n >= 1)
require("setupkoenv")
local document = require("ffi/mupdf").openDocument(path)
local page = document:openPage(n)
local dc = require("ffi/drawcontext").new()
local width,height = page:getSize(dc)
page:close()
document:close()
local code = assert(io.open("frontend/apps/reader/modules/readerzooming.lua","rb"))
local source = code:read("*all")
code:close()
local first = assert(source:find("function ReaderZooming:getZoom(",1,true))
local last = assert(source:find("\nend",first,true))
local method = source:sub(first,last+3):gsub("^function [^(]+%(", "return function(self, ", 1)
local run = assert(loadstring(method))()
local footer = {settings={reclaim_height=false}, getHeight=function() return 74 end}
local mock = {dimen={w=1264,h=1680},rotation=0,zoom_mode="pageheight",
    view={onBBoxUpdate=function() end},
    ui={view={footer_visible=true,footer=footer},document={
        getNativePageDimensions=function() return {w=width,h=height} end}}}
local reserved = run(mock,n)
footer.settings.reclaim_height = true
local shown = run(mock,n)
mock.ui.view.footer_visible = false
local hidden = run(mock,n)
assert(math.abs(reserved*height-1606)<0.0001 and math.abs(shown*height-1680)<0.0001 and shown==hidden)
footer.settings.reclaim_height=false
mock.ui.view.footer_visible=true
assert(run(mock,n)==reserved)
print(string.format('{"width":%.8f,"height":%.8f,"page":%d,"scale_ratio":%.9f,"full_height":1680,"old_height":1606,"reveal_preserved":true,"restore_identical":true}',width,height,n,shown/reserved))
