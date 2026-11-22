-- Measure original source spans on an installed KOReader without a display or user profile.
-- Run from its install directory with scratch, book, registered font, face and query.
local scratch,book,font,face,query = arg[1],arg[2],arg[3],arg[4],arg[5]
assert(scratch and scratch:sub(1,1)=="/" and not scratch:find("/%.%./") and not scratch:find("^/mnt/us/documents"))
assert(book and font and face and query)
os.setlocale("C","numeric")
require("setupkoenv")
local cre=require("libs/libkoreader-cre")
local lfs=require("libs/libkoreader-lfs")
local cache=scratch.."/cache"
if not lfs.attributes(cache) then assert(lfs.mkdir(cache)) end
cre.initCache(cache,0,true,1)
cre.registerFont(font)
local anchors
local function measure(percent)
    local view=cre.newDocView(610,900,1)
    view:setVisiblePageCount(1,false)
    local override = arg[6] == "left" and "p { text-align: left !important; }" or ""
    view:setStyleSheet("./data/epub.css",override)
    view:setIntProperty("crengine.render.block.rendering.flags",0x03375131)
    view:setIntProperty("crengine.style.cjk.width.scale.percent",percent)
    view:setHeaderInfo(0)
    view:setPageMargins(30,30,30,30)
    view:setFontSize(42)
    view:setFontFace(face)
    assert(view:loadDocument(book,false));view:renderDocument()
    if not anchors then
        local hits=assert(view:findAllText(query,false,false,16,true,0,255)); anchors={}
        for _,hit in ipairs(hits) do anchors[#anchors+1]={hit.start,hit["end"]} end
        assert(#anchors>0)
    end
    local widths,pages={},{}
    for _,a in ipairs(anchors) do
        assert(view:isXPointerInDocument(a[1]) and view:isXPointerInDocument(a[2]))
        view:gotoXPointer(a[1])
        local rects=assert(view:getWordBoxesFromPositions(a[1],a[2],true))
        local width=0
        for _,r in ipairs(rects) do width=width+r.x1-r.x0 end
        assert(width>0)
        widths[#widths+1]=width;pages[#pages+1]=view:getPageFromXPointer(a[1])
    end
    local data={widths=widths,pages=pages,count=view:getPages()}
    view:close();return data
end
local normal=measure(100);local wide=measure(150);local reset=measure(100)
assert(normal.count==reset.count)
for i,w in ipairs(normal.widths) do assert(w==reset.widths[i] and normal.pages[i]==reset.pages[i]) end
print("cjk-width "..require("json").encode({normal=normal,wide=wide,reset=true}))
