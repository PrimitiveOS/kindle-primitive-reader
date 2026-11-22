-- Compare the left edge of a uniquely resolved source passage with and without
-- horizontal publisher padding. Passage text and XPointers stay on the device.
local scratch, book = assert(arg[1]), assert(arg[2])
assert(scratch:match("^/mnt/us/%.primitive%-reader%-parity%.[%w]+/[%w%-]+/[%w%-]+$"))
assert(book:sub(1,18) == "/mnt/us/documents/" and not book:find("/%.%./"))
os.setlocale("C","numeric")
require("setupkoenv")
local cre = require("libs/libkoreader-cre")
local lfs = require("libs/libkoreader-lfs")
local cache = scratch .. "/cre-cache"
if not lfs.attributes(cache) then assert(lfs.mkdir(cache)) end
cre.initCache(cache,0,true,1)
cre.registerFont("/mnt/us/launcher/fonts/NotoSans-Regular.ttf")
local cases, counts = {}, {}
for row in io.lines(scratch .. "/phrases") do
    local chapter, shift, phrase = row:match("^(%d+)\t(%-?%d+)\t(.+)$")
    if chapter and (counts[chapter] or 0) < 5 and #cases < 16 then
        cases[#cases + 1] = {expected = tonumber(shift), text = phrase, chapter = tonumber(chapter)}
        counts[chapter] = (counts[chapter] or 0) + 1
    end
end
local zero_padding = "* {padding-left:0 !important; padding-right:0 !important}"
local zero_indent = "* {text-indent:0 !important}"
local function measure(override, anchors)
    local view = cre.newDocView(1264,1606,1)
    -- ReaderTypeset applies these flags when loading the user's chosen render mode.
    -- CREngine's standalone view defaults to Web; KOReader's reading UI defaults to Book.
    view:setIntProperty("crengine.render.block.rendering.flags", arg[4] == "web" and 0x7FFFFFFF or 0x03375131)
    view:setStyleSheet("./data/epub.css", override)
    view:setVisiblePageCount(1,false)
    view:setHeaderInfo(0)
    view:setPageMargins(54,54,54,54)
    view:setFontSize(42)
    view:setDefaultInterlineSpace(135)
    view:setFontFace("Noto Sans")
    assert(view:loadDocument(book,false))
    view:renderDocument()
    local rows = {}
    for i, case in ipairs(cases) do
        local ptr = anchors and anchors[i]
        if not anchors then
            local hits = view:findAllText(case.text:gsub("\194\173",""), false,false,2,true,0,255) or {}
            if #hits == 1 then
                ptr = {start=hits[1].start, finish=hits[1]["end"]}
            elseif #hits > 1 then
                -- A contents entry can repeat the heading it links to. Only
                -- accept it when the expected source chapter supplies one hit.
                hits = view:findAllText(case.text:gsub("\194\173",""),false,false,33,true,0,255) or {}
                if #hits < 33 then
                    for _, hit in ipairs(hits) do
                        local chapter = tonumber(hit.start:match("/DocFragment%[(%d+)%]/"))
                        if chapter == case.chapter then
                            if ptr then ptr = nil; break end
                            ptr = {start=hit.start, finish=hit["end"]}
                        end
                    end
                end
            end
        end
        if ptr and view:isXPointerInDocument(ptr.start) and view:isXPointerInDocument(ptr.finish) then
            local page = view:getPageFromXPointer(ptr.start)
            view:gotoPage(page)
            local rects = view:getWordBoxesFromPositions(ptr.start,ptr.finish,true) or {}
            if rects[1] then
                rows[i] = { start=ptr.start, finish=ptr.finish, x=rects[1].x0, page=page }
            end
        end
    end
    local pages = view:getPages()
    view:close()
    return rows,pages
end
local before, old_count = measure(zero_padding)
local after, new_count = measure("", before)
local reset, reset_count = measure(zero_padding, before)
assert(old_count == reset_count)
local natural, unindented, em
if arg[3] == "calibrate" then
    natural = measure(zero_padding .. zero_indent, before)
    unindented = measure(zero_indent, before)
    em = measure(zero_padding .. zero_indent .. "p {padding-left:1em !important}", before)
end
local spans = {}
for i, position in pairs(before) do
    assert(reset[i].page == position.page and reset[i].x == position.x)
    assert(after[i] and after[i].start == position.start)
    local extra = ""
    if em then
        extra = string.format(',"one_em_px":%d,"dx_without_text_indent":%d', em[i].x-natural[i].x, unindented[i].x-natural[i].x)
    end
    spans[#spans+1] = string.format('{"candidate":%d,"reader_dx":%d,"koreader_dx":%d,"control_x":%d,"published_x":%d%s}',i,cases[i].expected,after[i].x-position.x,position.x,after[i].x,extra)
end
print(string.format('{"original_pages":%d,"publisher_pages":%d,"spans":[%s],"source_positions_reset":true}',
    old_count,new_count,table.concat(spans,",")))
