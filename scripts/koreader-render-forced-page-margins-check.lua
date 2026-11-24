require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
local Device = require("device")
require("document/canvascontext"):init(Device)
local Typeset = require("apps/reader/modules/readertypeset")
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local source, cache, font, captures = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)
local width, height = 1264, 720
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

local modes = {"Legacy", "Flat", "Book", "Web"}
local cases = {
    {"retained-top", "Margin before previous", "Margin before next"},
    {"after-with-top", "Margin after previous", "Margin after next"},
    {"plain-margin", "Ordinary margin previous", "Ordinary margin next"},
    {"zero-top", "Zero margin previous", "Zero margin next"},
    {"kept-after", "Kept after previous", "Kept after next"},
}
local legacy = {{1,122,226}, {1,122,226}, {0,344,226}, {1,54,158}, {1,122,226}}
local enhanced = {{1,122,132}, {1,122,132}, {0,276,158}, {1,54,64}, {0,250,132}}
for mode=0,3 do
    local view=Cre.newDocView(width,height,1)
    local typeset=attach(view)
    typeset:setBlockRenderingMode(mode)
    configure(view)
    assert(view:loadDocument(source,false))
    view:renderDocument()
    assert(view:getPages() == (mode == 0 and 9 or 8))
    print(string.format("MODE %s pages=%d", modes[mode+1], view:getPages()))
    local anchors = {}
    local frame=BB.new(width,height,BB.TYPE_BB8)
    for index,entry in ipairs(cases) do
        local marks={}
        for index=2,3 do
            local phrase=entry[index]
            local hits = assert(view:findAllText(phrase,false,false,8,true,0,255))
            local hit = assert(hits[1])
            assert(view:getTextFromXPointers(hit.start,hit["end"]) == phrase)
            local page=view:getPageFromXPointer(hit.start)
            local doc_y=view:getPosFromXPointer(hit.start)
            view:gotoPage(page)
            local box = assert(view:getWordBoxesFromPositions(hit.start,hit["end"],true)[1])
            marks[index]={page=page,y=box.y0,doc_y=doc_y}
            anchors[phrase] = hit
        end
        local a,b=marks[2],marks[3]
        local expected = (mode == 0 and legacy or enhanced)[index]
        assert(b.page-a.page == expected[1] and b.y == expected[2])
        assert(b.doc_y-a.doc_y == expected[3])
        print(string.format("CASE %s|%s|delta=%d|before=%d|after=%d|pitch=%d",
            modes[mode+1],entry[1],b.page-a.page,a.y,b.y,b.doc_y-a.doc_y))
        if entry[1]=="retained-top" or entry[1]=="after-with-top" then
            view:drawCurrentPage(frame,false,false,false,false)
            frame:writePNG(captures.."/"..modes[mode+1].."-"..entry[1]..".png",false)
        end
    end
    if mode == 3 then
        for _, new_mode in ipairs{0, 1, 2, 3} do
            typeset:setBlockRenderingMode(new_mode)
            view:renderDocument()
            local hit = anchors["Margin before next"]
            assert(view:getTextFromXPointers(hit.start, hit["end"]) == "Margin before next")
            view:gotoXPointer(hit.start)
            assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true)[1].y0 == 122)
        end
    end
    frame:free()
    view:close()
end
Device:exit()
