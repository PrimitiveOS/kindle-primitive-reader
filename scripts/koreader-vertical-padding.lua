-- Measure screen and full-document coordinates of a private EPUB table.
-- Each private input line supplies a spine number, a numeric sample ID, and a phrase.
-- Only page numbers and pixel coordinates are written to stdout.
local scratch,book=assert(arg[1]),assert(arg[2])
assert(scratch:match("^/mnt/us/%.primitive%-reader%-parity%.[%w]+/[%w%-]+/[%w%-]+$"))
assert(book:sub(1,18)=="/mnt/us/documents/" and not book:find("/%.%./"))
os.setlocale("C","numeric")
require("setupkoenv")
local cre=require("libs/libkoreader-cre")
local lfs=require("libs/libkoreader-lfs")
local cache=scratch.."/cre-cache"
if not lfs.attributes(cache) then assert(lfs.mkdir(cache)) end
cre.initCache(cache,0,true,1)
cre.registerFont("/mnt/us/launcher/fonts/NotoSans-Regular.ttf")
local cases={}
for line in io.lines(scratch.."/phrases") do
    local chapter,index,text=line:match("^(%d+)\t(%-?%d+)\t(.+)$")
    if chapter and #cases<48 then cases[#cases+1]={chapter=tonumber(chapter),index=tonumber(index),text=text} end
end
local function measure(css,positions)
    local view=cre.newDocView(1264,1606,1)
    local property="crengine.render.block.rendering.flags"
    local flags=0x03375131 -- the normal Book mode in ReaderTypography
    view:setIntProperty(property,flags)
    assert(view:getIntProperty(property)==flags)
    view:setStyleSheet("./data/epub.css",css)
    view:setVisiblePageCount(1,false)
    view:setHeaderInfo(0)
    view:setPageMargins(54,54,54,54)
    view:setFontSize(42)
    view:setDefaultInterlineSpace(135)
    view:setFontFace("Noto Sans")
    assert(view:loadDocument(book,false))
    view:renderDocument()
    local out={}
    for i,c in ipairs(cases) do
        local p=positions and positions[i]
        if not positions then
            local hits=view:findAllText(c.text,false,false,32,true,0,255) or {}
            if #hits==1 then p={hits[1].start,hits[1]["end"]} end
        end
        if p then
            assert(view:isXPointerInDocument(p[1]))
            local page=view:getPageFromXPointer(p[1])
            view:gotoPage(page)
            local rect=assert(view:getWordBoxesFromPositions(p[1],p[2],true))[1]
            assert(rect and rect.x1>rect.x0)
            local doc_y,caret=view:getPosFromXPointer(p[1])
            out[i]={p[1],p[2],page,rect.x0,rect.y0,doc_y}
        end
    end
    view:close()
    return out
end
local original=measure("")
local no_edges=measure("td, th {padding-top:0 !important; padding-bottom:0 !important}",original)
local top_only=measure("td, th {padding-bottom:0 !important}",original)
local collapsed_outer=measure("table {padding-top:0 !important; padding-bottom:0 !important}",original)
local restore=measure("",original)
local rows={}
for i,p in pairs(original) do
    local n,t,c,r=no_edges[i],top_only[i],collapsed_outer[i],restore[i]
    for k=1,6 do assert(p[k]==r[k]) end
    local function val(a)return string.format("[%d,%d,%d,%d]",a[3],a[4],a[5],a[6])end
    rows[#rows+1]=string.format('{"source":%d,"published":%s,"cleared":%s,"top_only":%s,"table_cleared":%s}',cases[i].index,val(p),val(n),val(t),val(c))
end
print('{"spans":['..table.concat(rows,",")..'],"restored_source_and_boxes":true}')
