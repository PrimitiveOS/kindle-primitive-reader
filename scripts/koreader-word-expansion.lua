-- Measure original words in the installed CREngine at two expansion settings.
-- Use a bounded private test directory and a tab-separated list of passages
-- starting with the selected word. Output case number and box widths only.
local scratch, book = assert(arg[1]), assert(arg[2])
assert(scratch:match("^/mnt/us/%.primitive%-reader%-parity%.[%w]+/[%w%-]+%.[%w]+$"))
assert(book:sub(1, 18) == "/mnt/us/documents/" and not book:find("/%.%./"))
require("setupkoenv")
local lfs = require("libs/libkoreader-lfs")
local cre = require("libs/libkoreader-cre")
local cache = scratch .. "/cre-cache"
if not lfs.attributes(cache) then assert(lfs.mkdir(cache)) end
cre.initCache(cache, 0, true, 1)
cre.registerFont("/mnt/us/launcher/fonts/NotoSans-Regular.ttf")
local cases = {}
for row in io.lines(scratch .. "/expansion-cases.tsv") do
    local id, word, phrase = row:match("^(%d+)\t([^\t]+)\t([^\t]+)$")
    if id and #cases < 70 then
        cases[#cases + 1] = {id = tonumber(id), word = word, phrase = phrase}
    end
end
assert(#cases > 0)
local function measure(expansion)
    local view = cre.newDocView(1264, 1606, 1)
    view:setIntProperty("crengine.render.block.rendering.flags", 0x03375131) -- ReaderTypeset Book mode.
    view:setStyleSheet("./data/epub.css", "* { text-align: justify !important; hyphens: none !important }")
    view:setIntProperty("crengine.doc.embedded.fonts.enabled", 1)
    view:setIntProperty("crengine.style.max.added.letter.spacing.percent", expansion)
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(54, 54, 54, 54)
    view:setFontFace("Noto Sans")
    view:setFontSize(42)
    view:setDefaultInterlineSpace(135)
    assert(view:loadDocument(book, false))
    view:renderDocument()
    local widths = {}
    for _, case in ipairs(cases) do
        local hits = view:findAllText(case.phrase, true, false, 2, false, 0, 255) or {}
        if #hits == 1 then
            local source = hits[1].start
            local finish = view:getNextVisibleWordEnd(source)
            if finish and view:getTextFromXPointers(source, finish) == case.word then
                view:gotoPage(view:getPageFromXPointer(source))
                local boxes = view:getWordBoxesFromPositions(source, finish, true) or {}
                if #boxes == 1 then
                    widths[case.id] = boxes[1].x1 - boxes[1].x0
                end
            end
        end
    end
    view:close()
    return widths
end
local baseline, expanded = measure(0), measure(15)
for _, case in ipairs(cases) do
    local left, right = baseline[case.id], expanded[case.id]
    if left and right then
        print(string.format("stock-expansion %d %d %d", case.id, left, right))
    end
end
