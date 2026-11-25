-- Compare the same licensed font stored as sfnt and WOFF 1 in pinned CREngine.
-- Run in the isolated KOReader Mac profile described in embedded-font-families.md.
require("setupkoenv")
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local cache, noto, output, raw, packed = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4]), assert(arg[5])
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(noto)
local measures = {}
for _, specimen in ipairs({{"ttf", raw}, {"woff", packed}}) do
    measures[specimen[1]] = {}
    for enabled = 0, 1 do
        local view = Cre.newDocView(650, 900, 1)
        view:setStyleSheet("./data/epub.css", "")
        view:setVisiblePageCount(1, false)
        view:setHeaderInfo(0)
        view:setPageMargins(30, 30, 30, 30)
        view:setFontFace("Noto Sans")
        view:setFontSize(42)
        view:setIntProperty("crengine.doc.embedded.fonts.enabled", enabled)
        assert(view:loadDocument(specimen[2], false))
        view:renderDocument()
        local hit = assert((view:findAllText("Harbor lettering", true, false, 8, true, 0, 255) or {})[1])
        assert(view:getTextFromXPointers(hit.start, hit["end"]) == "Harbor lettering")
        view:gotoPage(view:getPageFromXPointer(hit.start))
        local rect = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true)[1])
        measures[specimen[1]][enabled] = rect.x1 - rect.x0
        if specimen[1] == "woff" then
            local frame = BB.new(650, 900, BB.TYPE_BB8)
            view:drawCurrentPage(frame, false, false, false, false)
            frame:writePNG(output .. "/woff-stock-" .. (enabled == 1 and "on" or "off") .. ".png", false)
            frame:free()
        end
        view:close()
    end
end
assert(measures.ttf[0] ~= measures.ttf[1])
assert(measures.ttf[0] == measures.woff[0])
assert(measures.ttf[1] == measures.woff[1])
print(("WOFF 1 and TTF match: disabled %d, enabled %d pixels"):format(measures.woff[0], measures.woff[1]))
