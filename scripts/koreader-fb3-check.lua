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
    elseif label == "harbor-continuation.fb3" then
        for i, title in ipairs({"Outer Survey Log", "Long Soundings", "Shore Register"}) do
            assert(contents[i].title == title, title)
        end
        local function locate(term)
            local hit = (view:findAllText(term, true, false, 16, true, 0, 255) or {})[1]
            assert(hit and view:getTextFromXPointers(hit.start, hit["end"]) == term, term)
            return view:getPageFromXPointer(hit.start), hit
        end
        local function visible_link(term)
            local page, hit = locate(term)
            view:gotoPage(page)
            local box = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true)[1])
            return view:getLinkFromPosition(math.floor((box.x0 + box.x1) / 2), math.floor((box.y0 + box.y1) / 2))
        end
        local first = locate("The early watch sets out")
        local later = locate("The final sounding records a silver buoy")
        local shore = locate("The shore register closes at dusk")
        local note = locate("The note records a calm sea")
        assert(first < later and later <= shore and shore <= note)
        local forward = visible_link("Find the final sounding")
        assert(forward == "#late", forward)
        view:gotoLink(forward)
        assert(view:getCurrentPage() == later)
        view:drawCurrentPage(frame, false, false, false, false)
        frame:writePNG(output .. "/stock-fb3-harbor-continuation.png", false)
        local back = visible_link("Return to the early watch")
        assert(back == "#opening", back)
        view:gotoLink(back)
        assert(view:getCurrentPage() == first)
    end
    view:gotoPage(math.min(2, pages))
    view:drawCurrentPage(frame, false, false, false, false)
    frame:free()
    print(("fb3-result\t%s\t%s\t%d\t%d"):format(label, format, pages, #contents))
    view:close()
end
