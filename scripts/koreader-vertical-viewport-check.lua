-- Run from the pinned KOReader build with its Lua source on LUA_PATH.
-- Pass an isolated directory, the EPUB and its extracted font.
local scratch, book, font = assert(arg[1]), assert(arg[2]), assert(arg[3])
os.setlocale("C", "numeric")
require("setupkoenv")
local ffi = require("ffi")
local lfs = require("libs/libkoreader-lfs")
local cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local cache = scratch .. "/vertical-viewport-cache"
assert(lfs.symlinkattributes(scratch, "mode") == "directory" and lfs.mkdir(cache))
cre.initCache(cache, 0, true, 1)
cre.registerFont(font)
local words = { "灯台", "港口" }

local function capture(width, height, writing_mode)
    local view = cre.newDocView(width, height, 1)
    local css = writing_mode and ("body { writing-mode: " .. writing_mode .. " !important; }") or ""
    view:setStyleSheet("./data/epub.css", css)
    view:setFontFace("Harbor Vertical Test Gothic")
    view:setFontSize(52)
    view:setVisiblePageCount(1, false)
    assert(view:loadDocument(book, false))
    view:renderDocument()
    local results = {}
    for _, word in ipairs(words) do
        local hits = assert(view:findAllText(word, false, false, 8, true, 0, 255))
        assert(#hits == 2)
        local last = hits[2]
        assert(view:getTextFromXPointers(last.start, last["end"]) == word)
        local page = view:getPageFromXPointer(last.start)
        view:gotoPage(page)
        local boxes = assert(view:getWordBoxesFromPositions(last.start, last["end"], true))
        assert(#boxes == 1)
        local box = boxes[1]
        assert(box.x1 - box.x0 > box.y1 - box.y0)
        local geometry = table.concat({page, box.x0, box.y0, box.x1, box.y1}, ",")
        local frame = BB.new(width, height, BB.TYPE_BB8)
        view:drawCurrentPage(frame, false, false, false, false)
        local bytes = ffi.string(frame.data, frame.stride * height)
        assert(bytes:find("[^\255]"))
        frame:free()
        results[word] = { bytes, geometry }
    end
    view:close()
    return results
end

for _, size in ipairs{{1264, 1680}, {1680, 1264}} do
    local width, height = unpack(size)
    local authored = capture(width, height)
    local horizontal = capture(width, height, "horizontal-tb")
    for _, word in ipairs(words) do
        assert(authored[word][1] == horizontal[word][1])
        assert(authored[word][2] == horizontal[word][2])
        print(string.format("%d x %d | %s | page/box %s | authored and horizontal rasters match",
            width, height, word, authored[word][2]))
    end
end
