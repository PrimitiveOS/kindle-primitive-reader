-- Reference horizontal coordinates in a particular table of a private EPUB.
-- Input lines contain a spine index, a Reader shift and an original phrase.
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
    local chapter,shift,text=line:match("^(%d+)\t(%-?%d+)\t(.+)$")
    if chapter and #cases<9 then cases[#cases+1]={chapter=tonumber(chapter),text=text} end
end
local ffi=require("ffi")
local BB=require("ffi/blitbuffer")
local width,height=1264,1606
local function capture(css,anchors)
    local view=cre.newDocView(width,height,1)
    local key="crengine.render.block.rendering.flags"
    view:setIntProperty(key,0x03375131)
    assert(view:getIntProperty(key)==0x03375131)
    view:setStyleSheet("./data/epub.css",css)
    view:setVisiblePageCount(1,false);view:setHeaderInfo(0)
    view:setPageMargins(54,54,54,54);view:setFontSize(42)
    view:setDefaultInterlineSpace(135);view:setFontFace("Noto Sans")
    assert(view:loadDocument(book,false));view:renderDocument()
    local bb=BB.new(width,height,BB.TYPE_BB8)
    local fields,pics={},{}
    for i,c in ipairs(cases) do
        local a=anchors and anchors[i] or nil
        if not a then
            local hits=view:findAllText(c.text,false,false,32,true,0,255) or {}
            assert(#hits==1);a={hits[1].start,hits[1]["end"]}
        end
        assert(view:isXPointerInDocument(a[1]))
        local page=view:getPageFromXPointer(a[1]);view:gotoPage(page)
        local rect=assert(view:getWordBoxesFromPositions(a[1],a[2],true))[1]
        assert(rect and rect.x1>rect.x0)
        fields[i]={a[1],a[2],page,rect.x0,rect.y0}
        if not pics[page] then
            view:drawCurrentPage(bb,false,false,false,false)
            assert(bb.stride==width)
            pics[page]=ffi.string(bb.data,width*height)
        end
    end
    bb:free();view:close();return fields,pics
end
local function delta(before,after)
    local count,rows,cols,max_h,max_v,pages=0,0,0,0,0,0
    for page,raw in pairs(before) do
        pages=pages+1
        local a,b=ffi.cast("const uint8_t*",raw),ffi.cast("const uint8_t*",assert(after[page]))
        local vruns=ffi.new("int[?]",width)
        for y=0,height-1 do
            local h=0
            for x=0,width-1 do
                local k=y*width+x
                if a[k]~=b[k] then
                    count=count+1;h=h+1;vruns[x]=vruns[x]+1
                    if h==64 then rows=rows+1 end
                    if vruns[x]==64 then cols=cols+1 end
                    if h>max_h then max_h=h end
                    if vruns[x]>max_v then max_v=vruns[x] end
                else h=0;vruns[x]=0 end
            end
        end
    end
    return string.format('{"pixels":%d,"horizontal_runs_64":%d,"vertical_runs_64":%d,"longest_horizontal":%d,"longest_vertical":%d,"pages":%d}',count,rows,cols,max_h,max_v,pages)
end
local base,published=capture("")
local transparent,clear=capture("table, tr, th, td, tbody, thead, tfoot {border-color:transparent !important}",base)
local function same_range(a,b)
    return a[1]==b[1] and a[2]==b[2] and a[3]==b[3] and a[4]==b[4] and a[5]==b[5]
end
for i,a in ipairs(base) do assert(same_range(a,transparent[i])) end
local difference=delta(published,clear)
local hidden=capture("table, tr, th, td, tbody, thead, tfoot {border-style:hidden !important}",base)
local sep,sep_ink=capture("table {border-collapse:separate !important; border-spacing:0 !important}",base)
local clear_sep,sep_clear=capture("table {border-collapse:separate !important; border-spacing:0 !important} table, th,td,tr,tbody,thead,tfoot {border-color:transparent !important}",base)
for i,a in ipairs(sep) do assert(same_range(a,clear_sep[i])) end
local reset,restored=capture("",base)
for i,a in ipairs(base) do assert(same_range(a,reset[i]));assert(published[a[3]]==restored[a[3]]) end
local geom={}
for i,a in ipairs(base) do geom[i]=string.format('{"candidate":%d,"published":%d,"transparent":%d,"hidden":%d,"separate":%d}',i,a[4],transparent[i][4],hidden[i][4],sep[i][4]) end
print('{"spans":['..table.concat(geom,",")..'],"collapsed":'..difference..',"separate":'..delta(sep_ink,sep_clear)..',"restored_source_and_boxes":true}')
