-- Read an EPUB through installed CREngine and decode its declared GIFs with KOReader's image inspector.
-- Invoke from the KOReader installation: ./luajit this-script scratch book.epub reference.json
local scratch, book, reference = arg[1], arg[2], arg[3]
assert(scratch and book and reference and not arg[4])
os.setlocale("C", "numeric")
require("setupkoenv")
local ffi = require("ffi")
local cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local Pic = require("ffi/pic")
local lfs = require("libs/libkoreader-lfs")
local md5 = require("ffi/sha2").md5
assert(lfs.symlinkattributes(scratch, "mode") == "directory")
local cache = scratch .. "/gif-cre-cache"
if not lfs.attributes(cache) then assert(lfs.mkdir(cache)) end
assert(lfs.symlinkattributes(cache,"mode")=="directory")
local ref = assert(io.open(reference,"rb"))
local inventory = require("json").decode(ref:read("*all"))
ref:close()
cre.initCache(cache,0,true,1)
cre.registerFont("/mnt/us/launcher/fonts/NotoSans-Regular.ttf")
local width,height = 1264,1606
local view = cre.newDocView(width,height,1)
view:setStyleSheet("./data/epub.css")
view:setFontSize(42)
view:setFontFace("Noto Sans")
view:setPageMargins(54,54,54,54)
view:setVisiblePageCount(1,false)
view:setHeaderInfo(0)
assert(view:loadDocument(book,false))
view:renderDocument()
local count=0
local target_bytes
for name,expected in pairs(inventory.assets) do
    local bytes=assert(view:getDocumentFileContent(name),"CREngine could not retrieve a declared resource")
    local gif=assert(Pic.openGIFDocumentFromData(bytes,#bytes))
    assert(gif:getPages()==expected.frames,"Frame count differs")
    local page=gif:openPage(1)
    local bb=page.image_bb
    assert(bb:getWidth()==expected.width and bb:getHeight()==expected.height,"Canvas size differs")
    local gray=ffi.new("uint8_t[?]",expected.width*expected.height)
    for y=0, expected.height-1 do
        for x=0, expected.width-1 do
            local p=bb:getPixel(x,y)
            gray[y*expected.width+x]=math.floor((2126*p:getR()+7152*p:getG()+722*p:getB())/10000)
        end
    end
    assert(md5(ffi.string(gray,expected.width*expected.height))==expected.md5,"Independent first-frame pixels differ")
    if name==inventory.sample.member then target_bytes=bytes end
    page:close()
    gif:close()
    count=count+1
end
local target = assert(target_bytes)
local page = view:getPageFromXPointer(inventory.sample.xpointer)
local frame = BB.new(width,height,BB.TYPE_BB8)
local found = false
for _,offset in ipairs({0,1,-1,2,-2}) do
    local n = page + offset
    if n>=1 and n<=view:getPages() then
        view:gotoPage(n)
        view:drawCurrentPage(frame,false,false,false,false)
        for y=64,height-40,35 do
            for x=64,width-40,35 do
                local data,size=view:getImageDataFromPosition(x,y,false)
                if data then
                    found=(size==#target and ffi.string(data,size)==target)
                    ffi.C.free(data)
                    if found then break end
                end
            end
            if found then break end
        end
    end
    if found then break end
end
view:close()
frame:free()
assert(found,"Could not hit the chosen image on its rendered CREngine page")
print(string.format("gif_koreader assets=%d first_frame_pixels_match=true page_image_hit=true",count))
