-- Run inside the pinned native KOReader Mac runtime with an isolated KO_HOME.
local cache, font, output = assert(arg[1]), assert(arg[2]), assert(arg[3])
require("setupkoenv")
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)
for i = 4, #arg do
    local path = arg[i]
    local width, height = 650, 900
    local view = Cre.newDocView(width, height, 1)
    view:setIntProperty("crengine.render.block.rendering.flags", 0x03375131)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setStyleSheet("./data/epub.css")
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(30, 30, 30, 30)
    view:setFontFace("Noto Sans")
    view:setFontSize(26)
    local label = assert(path:match("([^/]+)$"))
    assert(view:loadDocument(path, false), label .. ": open failed")
    local format = view:getDocumentFormat()
    assert(format:find("FB3", 1, true), label .. ": " .. format)
    view:renderDocument()
    local pages = view:getPages()
    local contents = view:getToc()
    assert(pages > 0)
    local frame = BB.new(width, height, BB.TYPE_BB8)
    view:gotoPage(1)
    view:drawCurrentPage(frame, false, false, false, false)
    if label == "harbor-survey.fb3" then
        assert(contents[1].title == "Morning Soundings")
        assert(contents[2].title == "Ridge Observations")
        for _, term in ipairs({"The silver ridge", "The spring tide"}) do
            local hit = (view:findAllText(term, true, false, 16, true, 0, 255) or {})[1]
            assert(hit and view:getTextFromXPointers(hit.start, hit["end"]) == term, term)
        end
        view:gotoPage(contents[1].page)
        view:drawCurrentPage(frame, false, false, false, false)
        frame:writePNG(output .. "/stock-fb3-harbor-survey.png", false)
    end
    view:gotoPage(math.min(2, pages))
    view:drawCurrentPage(frame, false, false, false, false)
    frame:free()
    print(("fb3-result\t%s\t%s\t%d\t%d"):format(label, format, pages, #contents))
    view:close()
end
