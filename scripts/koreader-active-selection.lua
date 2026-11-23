-- Compare CREngine's live selection background with an unmarked page of the supplied book.
-- Run inside the KOReader install, with a private scratch folder and an authored test EPUB.
local scratch, book = assert(arg[1]), assert(arg[2])
require("setupkoenv")
local BB = require("ffi/blitbuffer")
local cre = require("libs/libkoreader-cre")
local lfs = require("libs/libkoreader-lfs")
local cache = scratch.."/cache"
assert(lfs.mkdir(cache))
cre.initCache(cache,0,true,1)
cre.registerFont("/mnt/us/launcher/fonts/NotoSans-Regular.ttf")
local view=cre.newDocView(1264,1606,1)
view:setStyleSheet("./data/epub.css")
view:setFontFace("Noto Sans")
view:setFontSize(42)
view:setPageMargins(54,54,54,54)
view:setVisiblePageCount(1,false)
view:setHeaderInfo(0)
assert(view:loadDocument(book,false))
view:renderDocument()
view:gotoPage(view:getPages())
local settings={}
local fn=assert(io.open("frontend/apps/reader/modules/readerhighlight.lua","rb"))
local source=fn:read("*all");fn:close()
local method=assert(source:match("(function ReaderHighlight:setSelectionColor%b()%s*.-)\nend\n")).."\nend"
local scope={ReaderHighlight={},Blitbuffer=BB,Screen={},G_reader_settings={
    isTrue=function()return false end,readSetting=function(_,k)return settings[k]end}}
setmetatable(scope,{__index=_G});local chunk=assert(loadstring(method));setfenv(chunk,scope);chunk()
local module={ ui={paging=false},view={highlight={saved_drawer="lighten"}},document={_document=view}}
local base=BB.new(1264,1606,BB.TYPE_BB8)
local selected=BB.new(1264,1606,BB.TYPE_BB8)
view:clearSelection()
view:drawCurrentPage(base,false,false,false,false)
local hit,rect
for y=80,1400,14 do
    for x=100,1100,25 do
        local t=view:getTextFromPositions(x,y,x,y,false,false)
        if t and t.text and #t.text>=4 then
            local b=view:getWordBoxesFromPositions(t.pos0,t.pos1)
            if #b==1 and b[1].y0>=20 and b[1].y1<1510 then hit={x,y};rect=b[1]; break end
        end
    end
    if hit then break end
end
assert(hit)
for _,opacity in ipairs({0,5,10,15,20,30,35,50,60,65,90,95,100}) do
    for _,height in ipairs({0,49,100}) do
        settings.highlight_selection_lighten_factor=opacity/100
        settings.highlight_height_pct=height
        scope.ReaderHighlight.setSelectionColor(module)
        assert(view:getTextFromPositions(hit[1],hit[2],hit[1],hit[2]))
        view:drawCurrentPage(selected,false,false,false,false)
        local n,rows,shade=0,0,255
        for y=rect.y0-2,rect.y1+2 do
            local changed=false
            for x=rect.x0,rect.x1-1 do
                local original=base:getPixel(x,y):getColor8().a
                local v=selected:getPixel(x,y):getColor8().a
                if original==255 and v~=255 then n=n+1; changed=true; shade=math.min(shade,v) end
            end
            if changed then rows=rows+1 end
        end
        print("reflow-selection",opacity,height,n,rows,shade)
        view:clearSelection()
    end
end
base:free();selected:free();view:close()
