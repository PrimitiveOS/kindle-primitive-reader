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
local views={
  published="",
  table_unpadded="table {padding-left:0 !important; padding-right:0 !important}",
  cells_unpadded="td,th {padding-left:0 !important; padding-right:0 !important}",
  unpadded="* {padding-left:0 !important; padding-right:0 !important}",
  narrow="table {width:60% !important}",
  modest="table {width:80% !important}",
  full="table {width:100% !important}",
  auto_cells="td, th {width:auto !important}",
  borderless="table, th, td {border:0 none !important}",
  separate="table {border-collapse:separate !important; border-spacing:0 !important}",
  web_narrow="table {width:60% !important}",
  web_modest="table {width:80% !important}",
  web_published="",
  web_full="table {width:100% !important}",
}
local function measure(css,positions,mode)
    local view=cre.newDocView(1264,1606,1)
    local property="crengine.render.block.rendering.flags"
    local flags=mode and mode:sub(1,4)=="web_" and 0x7fffffff or 0x03375131
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
            local _,caret=view:getPosFromXPointer(p[1])
            local cell=p[1]:match("^(.-/td%[%d+%])[/%.]") or p[1]:match("^(.-/td)[/%.]")
            assert(cell and view:isXPointerInDocument(cell),"Missing containing cell")
            local _,cell_x=view:getPosFromXPointer(cell)
            out[i]={p[1],p[2],page,rect.x0,rect.x1,caret+54,cell_x+54}
        end
    end
    view:close()
    return out
end
local original=measure(views.published)
local function geometry(key,p)
    return string.format('"%s":{"ink":%d,"caret":%d,"cell":%d}',key,p[4],p[6],p[7])
end
local rows={}
for i,p in pairs(original) do rows[i]={string.format('"candidate":%d',i),geometry("published",p)} end
for _,mode in ipairs({"table_unpadded","cells_unpadded","unpadded","narrow","modest","full","auto_cells","borderless","separate","web_narrow","web_modest","web_published","web_full"}) do
    local found=measure(views[mode],original,mode)
    for i in pairs(original) do rows[i][#rows[i]+1]=geometry(mode,assert(found[i])) end
end
local reset=measure(views.published,original)
local collected={}
for i=1,#cases do
    local p=original[i]
    if p then
        assert(reset[i][3]==p[3] and reset[i][4]==p[4] and reset[i][5]==p[5] and reset[i][6]==p[6] and reset[i][7]==p[7])
        collected[#collected+1]="{"..table.concat(rows[i],",").."}"
    end
end
print('{"spans":['..table.concat(collected,",")..'],"restored_source_and_boxes":true}')
