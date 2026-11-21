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
local pages = view:getPageMap() or {}
for _, item in ipairs(pages) do
    assert(item.page > 0 and item.xpointer and item.label)
    local label = item.label:gsub(".", function(c) return string.format("%02x", string.byte(c)) end)
    print("label " .. label)
end
print("count " .. #pages)
view:close()
