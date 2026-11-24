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

local modes = {"Legacy", "Flat", "Book", "Web"}
local pairs = {
    {"Natural first","Natural second"}, {"Before joined","Joined first"},
    {"Joined first","Joined last"}, {"Joined last","After joined"},
    {"Before signed","Signed first"}, {"Signed last","After signed"},
    {"Before barrier","Barrier first"}, {"Barrier last","After barrier"},
    {"Before text","Intervening parent text"}, {"Intervening parent text","Text child"},
    {"Before border","Border child"}, {"Border child","After border"},
    {"Before nested","Nested child"}, {"Nested child","After nested"},
}
local expected = {
    {64,190,94,178,111,79,150,157,96,98,150,157,149,137},
    {64,120,82,108,93,64,135,137,84,98,135,137,102,94},
    {64,120,82,108,93,64,135,137,84,98,135,137,102,94},
    {64,120,82,108,93,51,135,137,84,98,135,137,102,94},
}
for mode=0,3 do
    local view=Cre.newDocView(width,height,1)
    local typeset=attach(view)
    typeset:setBlockRenderingMode(mode)
    configure(view)
    assert(view:loadDocument(source,false))
    view:renderDocument()
    assert(view:getPages() == 2)
    print(string.format("MODE %s pages=%d", modes[mode+1], view:getPages()))
    local frame=BB.new(width,height,BB.TYPE_BB8)
    local observed={}
    for index,pair in ipairs(pairs) do
        for _,phrase in ipairs(pair) do
            local hits = assert(view:findAllText(phrase,false,false,8,true,0,255))
            local hit = assert(hits[1])
            assert(view:getTextFromXPointers(hit.start,hit["end"]) == phrase)
            local page=view:getPageFromXPointer(hit.start)
            local y=view:getPosFromXPointer(hit.start)
            observed[phrase]={y=y,page=page,hit=hit}
        end
        local a,b=observed[pair[1]],observed[pair[2]]
        assert(b.y-a.y == expected[mode+1][index])
        print(string.format("GAP %s|%s to %s|pitch=%d|pages=%d,%d",
           modes[mode+1],pair[1],pair[2],b.y-a.y,a.page,b.page))
    end
    for page=1,view:getPages() do
        view:gotoPage(page)
        view:drawCurrentPage(frame,false,false,false,false)
        frame:writePNG(captures.."/"..modes[mode+1].."-"..page..".png",false)
    end
    if mode == 3 then
        local first = observed["Nested child"].hit
        local last = observed["After nested"].hit
        local original = observed["Nested child"].y
        for _, next_mode in ipairs{0, 1, 2, 3} do
            typeset:setBlockRenderingMode(next_mode)
            view:renderDocument()
            assert(view:getTextFromXPointers(first.start, first["end"]) == "Nested child")
            assert(view:getPageFromXPointer(first.start) == 2)
            assert(view:getPosFromXPointer(last.start) - view:getPosFromXPointer(first.start)
                == expected[next_mode+1][#pairs])
        end
        assert(view:getPosFromXPointer(first.start) == original)
    end
    frame:free()
    view:close()
end
Device:exit()
