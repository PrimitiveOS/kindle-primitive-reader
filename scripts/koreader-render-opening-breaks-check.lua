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
    {"direct-before", "Direct before previous", "Direct before next"},
    {"direct-after", "Direct after previous", "Direct after next"},
    {"nested-before", "Nested before previous", "Nested before next"},
    {"nested-after", "Nested after previous", "Nested after next"},
    {"adjacent-before", "Adjacent before previous", "Adjacent before next"},
    {"adjacent-after", "Adjacent after previous", "Adjacent after next"},
    {"plain-after", "Standalone after previous", "Standalone after next"},
    {"plain-before", "Standalone before previous", "Standalone before next"},
}
local legacy = {1, 1, 1, 1, 0, 0, 1, 1}
local enhanced = {1, 0, 1, 0, 1, 0, 1, 1}
for mode=0,3 do
    local view=Cre.newDocView(width,height,1)
    local typeset=attach(view)
    typeset:setBlockRenderingMode(mode)
    configure(view)
    assert(view:loadDocument(source,false))
    view:renderDocument()
    assert(view:getPages() == (mode == 0 and 14 or 13))
    print(string.format("MODE %s pages=%d", modes[mode+1], view:getPages()))
    local frame=BB.new(width,height,BB.TYPE_BB8)
    local anchors = {}
    for case,entry in ipairs(cases) do
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
            anchors[phrase] = hit
            marks[index]={page=page,y=box.y0,doc_y=doc_y}
        end
        local a,b=marks[2],marks[3]
        local delta = (mode == 0 and legacy or enhanced)[case]
        assert(b.page-a.page == delta)
        assert(a.y == 118 and b.y == (delta == 1 and 54 or 182))
        assert(b.doc_y-a.doc_y == 64)
        print(string.format("CASE %s|%s|delta=%d|before=%d|after=%d|pitch=%d",
            modes[mode+1],entry[1],b.page-a.page,a.y,b.y,b.doc_y-a.doc_y))
        if mode == 3 and (entry[1]=="nested-before" or entry[1]=="nested-after") then
            view:drawCurrentPage(frame,false,false,false,false)
            frame:writePNG(captures.."/"..modes[mode+1].."-"..entry[1]..".png",false)
        end
    end
    if mode == 3 then
        local before = anchors["Nested before next"]
        local after = anchors["Nested after next"]
        for _, next_mode in ipairs{0, 1, 2, 3} do
            typeset:setBlockRenderingMode(next_mode)
            view:renderDocument()
            assert(view:getTextFromXPointers(before.start, before["end"]) == "Nested before next")
            assert(view:getTextFromXPointers(after.start, after["end"]) == "Nested after next")
            view:gotoXPointer(before.start)
            assert(view:getWordBoxesFromPositions(before.start, before["end"], true)[1].y0 == 54)
            view:gotoXPointer(after.start)
            assert(view:getWordBoxesFromPositions(after.start, after["end"], true)[1].y0 == (next_mode == 0 and 54 or 182))
        end
    end
    frame:free()
    view:close()
end
Device:exit()
