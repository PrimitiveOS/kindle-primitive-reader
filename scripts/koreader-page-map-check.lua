-- Run under the installed KOReader. Read a publication with an isolated CRE cache.
local scratch, document = arg[1], arg[2]
assert(scratch and scratch:match("^/mnt/us/%.primitive%-reader%-parity%.[%w]+/page%-map%.[%w]+$"))
assert(document and document:match("^/mnt/us/documents/") and not document:find("/%.%./"))
os.setlocale("C", "numeric")
require("setupkoenv")
local cre = require("libs/libkoreader-cre")
cre.initCache(scratch, 0, true, 1)
cre.registerFont("/mnt/us/launcher/fonts/NotoSans-Regular.ttf")
local view = cre.newDocView(1264, 1606, 1)
view:setStyleSheet("./data/epub.css")
view:setFontFace("Noto Sans")
view:setFontSize(42)
assert(view:loadDocument(document, false))
view:renderDocument()
local stride = arg[3] and assert(tonumber(arg[3]))
if stride then
    assert(stride == math.floor(stride) and stride >= 500 and stride <= 3000)
    view:buildSyntheticPageMap(stride)
end
local pages = view:getPageMap() or {}
for index, item in ipairs(pages) do
    assert(item.page > 0 and item.xpointer and item.label)
    if stride then
        assert(tonumber(item.label) == index and not item.xpointer:find("[\r\n]"))
        print("point " .. item.xpointer)
    else
        local label = item.label:gsub(".", function(c) return string.format("%02x", string.byte(c)) end)
        print("label " .. label)
    end
end
print("count " .. #pages)
view:close()
