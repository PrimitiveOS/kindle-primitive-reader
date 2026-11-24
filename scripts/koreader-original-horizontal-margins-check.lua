-- Use a private TSV with one-based chapter, expected pixel shift and unique text.
-- Log aggregate coordinates only. The comparison keeps the original file unopened for writing.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
local Device = require("device")
require("document/canvascontext"):init(Device)
local Typeset = require("apps/reader/modules/readertypeset")
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local source, cache, font, manifest = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)
local width, height = 1264, 1606
local property = "crengine.render.block.rendering.flags"
local function configure(view)
    view:setStyleSheet("./data/epub.css", "")
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(54, 54, 54, 54)
    view:setFontSize(40)
    view:setDefaultInterlineSpace(120)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setFontFace("Noto Sans")
end
local function attach(view)
    local updates = 0
    local typeset = setmetatable({ ui = {
        document = { setBlockRenderingFlags = function(_, flags) view:setIntProperty(property, flags) end },
        handleEvent = function(_, event) assert(event.handler == "onUpdatePos"); updates = updates + 1 end,
    }}, { __index = Typeset })
    return typeset, function() return updates end
end

local function configure(view, override)
    view:setStyleSheet("./data/epub.css", override)
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(54, 54, 54, 54)
    view:setFontSize(40)
    view:setDefaultInterlineSpace(120)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setFontFace("Noto Sans")
end
local function attach(view)
    local updates = 0
    local typeset = setmetatable({ ui = {
        document = { setBlockRenderingFlags = function(_, flags) view:setIntProperty(property, flags) end },
        handleEvent = function(_, event) assert(event.handler == "onUpdatePos"); updates = updates + 1 end,
    }}, { __index = Typeset })
    return typeset, function() return updates end
end


local cases = {}
for line in io.lines(manifest) do
    local chapter, dx, phrase = line:match("^(%d+)	(%d+)	(.+)$")
    assert(chapter and #cases < 20)
    cases[#cases+1] = {chapter=tonumber(chapter), dx=tonumber(dx), phrase=phrase}
end
assert(#cases > 0)
local reset = "p {margin-left:0!important; margin-right:0!important}"
local function sample(override, anchors)
    local view = Cre.newDocView(width, height, 1)
    local typeset = attach(view)
    typeset:setBlockRenderingMode(3)
    configure(view, override)
    assert(view:loadDocument(source, false))
    view:renderDocument()
    local values = {}
    for index, case in ipairs(cases) do
        local hit = anchors and anchors[index]
        if not hit then
            local hits = view:findAllText(case.phrase, false, false, 33, true, 0, 255) or {}
            assert(#hits < 33)
            for _, possible in ipairs(hits) do
                local chapter = tonumber(possible.start:match("/DocFragment%[(%d+)%]/"))
                if chapter == case.chapter then
                    assert(not hit, "ambiguous source passage")
                    hit = possible
                end
            end
        end
        assert(hit and view:isXPointerInDocument(hit.start) and view:isXPointerInDocument(hit["end"]),
            "source passage missing")
        local text = assert(view:getTextFromXPointers(hit.start, hit["end"]))
        assert(#text > 0)
        local page = view:getPageFromXPointer(hit.start)
        view:gotoPage(page)
        local word = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true)[1])
        values[index] = {hit=hit, text=text, x=word.x0, page=page}
    end
    view:close()
    return values
end
local original = sample("")
local anchors = {}
for i, value in ipairs(original) do anchors[i] = value.hit end
local control = sample(reset, anchors)
local restored = sample("", anchors)
for index, value in ipairs(original) do
    assert(value.x == restored[index].x and value.page == restored[index].page)
    assert(value.text == control[index].text and value.text == restored[index].text)
    local shift = value.x - control[index].x
    print(string.format("PRIVATE MARGIN index=%d shift=%d restored=true", index, shift))
    assert(shift == cases[index].dx)
end
Device:exit()
