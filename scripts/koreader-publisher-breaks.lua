-- Compare original text positions on either side of publisher page breaks.
local scratch, book, list = assert(arg[1]), assert(arg[2]), assert(arg[3])
os.setlocale("C", "numeric")
require("setupkoenv")
local lfs = require("libs/libkoreader-lfs")
local cre = require("libs/libkoreader-cre")
local cache = scratch .. "/page-break-cache"
assert(lfs.symlinkattributes(scratch, "mode") == "directory" and lfs.mkdir(cache))
cre.initCache(cache, 0, true, 1)
cre.registerFont("/mnt/us/launcher/fonts/NotoSans-Regular.ttf")
local cases = {}
for row in io.lines(list) do
    local kind, prev, next_word = row:match("^([^\t]+)\t([^\t]+)\t([^\t]+)$")
    assert(kind == "break-before" or kind == "break-after" or kind == "break-inside")
    cases[#cases+1] = {prev, next_word}
end
for _, mode in ipairs({"source", "auto"}) do
    local view = cre.newDocView(1264, 1606, 1)
    local css = mode == "source" and "" or "* {page-break-before:auto !important; page-break-after:auto !important; page-break-inside:auto !important}"
    view:setStyleSheet("./data/epub.css", css)
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(54, 54, 54, 54)
    view:setFontSize(42)
    view:setFontFace("Noto Sans")
    assert(view:loadDocument(book, false))
    view:renderDocument()
    for i, pair in ipairs(cases) do
        assert(view:isXPointerInDocument(pair[1]) and view:isXPointerInDocument(pair[2]))
        print(string.format("break %d %s %d %d", i, mode,
            view:getPageFromXPointer(pair[1]), view:getPageFromXPointer(pair[2])))
    end
    view:close()
end
print("completed")
