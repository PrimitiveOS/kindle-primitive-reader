-- Compare the same source passages with and without protected paragraph ends.
-- Pass the original EPUB and private scratch directory containing tab-separated
-- candidate passages. The output contains page and passage counts.
local directory, book = assert(arg[1]), assert(arg[2])
assert(directory:match("^/mnt/us/%.primitive%-reader%-parity%.[%w]+/[%w%-]+/[%w%-]+$"))
assert(book:sub(1, 18) == "/mnt/us/documents/" and not book:find("/%.%./"))
os.setlocale("C", "numeric")
require("setupkoenv")
local cre = require("libs/libkoreader-cre")
local lfs = require("libs/libkoreader-lfs")
local cache = directory .. "/cre-cache"
if not lfs.attributes(cache) then assert(lfs.mkdir(cache)) end
cre.initCache(cache, 0, true, 1)
cre.registerFont("/mnt/us/launcher/fonts/NotoSans-Regular.ttf")

local candidates, per_chapter = {}, {}
for row in io.lines(directory .. "/passages") do
    local chapter, left, right = row:match("^(%d+)\t([^\t]+)\t([^\t]+)$")
    if chapter and (per_chapter[chapter] or 0) < 2 and #candidates < 10 then
        candidates[#candidates + 1] = {left, right}
        per_chapter[chapter] = (per_chapter[chapter] or 0) + 1
    end
end

local function locate(view, text)
    text = text:gsub("\194\173", "") -- Ignore discretionary break opportunities.
    local function unique(phrase)
        local hits = view:findAllText(phrase, false, false, 2, true, 0, 255) or {}
        if #hits == 1 then return hits[1].start end
    end
    local full_line = unique(text)
    if full_line then return full_line end

    -- Line ends sometimes meet an inline element or a discretionary hyphen.
    -- Look up an interior phrase from the same source line in that case.
    local words = {}
    for word in text:gmatch("%S+") do words[#words + 1] = word end
    if #words >= 7 then
        local first = math.floor((#words - 5) / 2) + 1
        return unique(table.concat(words, " ", first, first + 4))
    end
end

local function measure(relaxed, anchors)
    local view = cre.newDocView(1264, 1606, 1)
    -- Normal KOReader Book mode, set by ReaderTypeset before opening a book.
    view:setIntProperty("crengine.render.block.rendering.flags", 0x03375131)
    local override = relaxed and "* {orphans:1 !important; widows:1 !important}" or ""
    view:setStyleSheet("./data/epub.css", override)
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(54, 54, 54, 54)
    view:setFontSize(42)
    view:setDefaultInterlineSpace(135)
    view:setFontFace("Noto Sans")
    assert(view:loadDocument(book, false))
    view:renderDocument()

    local count = view:getPages()
    assert(count > 0 and count <= 5000)
    local starts, found = {}, {}
    for n = 1, count do starts[n] = assert(view:getPageXPointer(n)) end
    for i, pair in ipairs(candidates) do
        local row = anchors and anchors[i]
        if not anchors then row = {locate(view, pair[1]), locate(view, pair[2])} end
        if row and row[1] and row[2] then
            assert(view:isXPointerInDocument(row[1]) and view:isXPointerInDocument(row[2]))
            found[i] = {
                row[1], row[2], view:getPageFromXPointer(row[1]), view:getPageFromXPointer(row[2]),
            }
        end
    end
    view:close()
    return starts, found
end

local relaxed, original_anchors = measure(true)
local published, placed = measure(false, original_anchors)
local reset = measure(true, original_anchors)
assert(#reset == #relaxed)
local changed = math.abs(#relaxed - #published)
for i, start in ipairs(relaxed) do
    assert(reset[i] == start)
    if i <= #published and start ~= published[i] then changed = changed + 1 end
end
local aligned, joined, apart = 0, 0, 0
for i, before in pairs(original_anchors) do
    local after = assert(placed[i])
    aligned = aligned + 1
    if before[3] ~= before[4] and after[3] == after[4] then joined = joined + 1 end
    if after[3] ~= after[4] then apart = apart + 1 end
end
print(string.format('{"relaxed_pages":%d,"published_pages":%d,"changed_page_starts":%d,' ..
    '"aligned_passage_pairs":%d,"rejoined_passage_pairs":%d,"stock_pairs_still_apart":%d,' ..
    '"reset_page_starts_identical":true}', #relaxed, #published, changed, aligned, joined, apart))
