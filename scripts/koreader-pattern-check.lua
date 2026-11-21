-- Offscreen comparator for installed KOReader CREngine. Output contains only counts and a checksum.
local scratch, document, pattern, mode, search_type = arg[1], arg[2], arg[3], arg[4], arg[5]
assert(search_type == nil or search_type == "literal")
local regex = search_type ~= "literal"
assert(scratch and scratch:sub(1,1) == "/" and scratch ~= "/mnt/us/documents" and scratch:sub(1,18) ~= "/mnt/us/documents/" and not scratch:find("/%.%./"))
assert(document and document:sub(1,1) == "/" and not document:find("/%.%./"))
assert(mode == "exact" or mode == "insensitive")
os.setlocale("C", "numeric")
require("setupkoenv")
local lfs = require("libs/libkoreader-lfs")
local cre = require("libs/libkoreader-cre")
local cache = scratch .. "/cache"
if not lfs.attributes(cache) then assert(lfs.mkdir(cache)) end
cre.initCache(cache, 0, true, 1)
cre.registerFont("/mnt/us/launcher/fonts/NotoSans-Regular.ttf")
local view = cre.newDocView(1264, 1606, 1)
view:setStyleSheet("./data/epub.css")
view:setFontFace("Noto Sans")
view:setFontSize(42)
assert(view:loadDocument(document, false))
view:renderDocument()
if regex then assert(view:checkRegex(pattern) == 0) end
local hits = view:findAllText(pattern, mode == "insensitive", regex, 100, true, 0, regex and 1 or 255) or {}
local a, b, characters = 1, 0, 0
local function add(byte) a = (a+byte)%65521; b=(b+a)%65521 end
for i=1,math.min(100,#hits) do
    local text = assert(hits[i].matched_text)
    assert(hits[i].start and hits[i]["end"])
    for j=1,#text do
        local c = string.byte(text,j)
        if c < 128 or c >= 192 then characters=characters+1 end
        add(c)
    end
    add(0)
end
view:close()
print(string.format('{"count":%d,"characters":%d,"ordered_text_checksum":"%04x%04x"}',math.min(100,#hits),characters,b,a))
