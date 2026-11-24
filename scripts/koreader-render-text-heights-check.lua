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

local passages = {"Natural height", "Control follows", "Tall height", "Tall follows",
    "Short height", "Short continuation", "Short follows", "Percent height", "Percent follows",
    "Web wide and tall", "Width follows", "Page fill height", "Last line"}
local shades = {["Natural height"]=235,["Tall height"]=224,["Short height"]=214,
    ["Percent height"]=204,["Web wide and tall"]=194,["Page fill height"]=184}
local modes = {"Legacy","Flat","Book","Web"}
if arg[5] then height = assert(tonumber(arg[5])) end
assert(height == 1606 or height == 690)
local compact = height == 690
local web_heights = {["Tall height"] = 164, ["Web wide and tall"] = 153,
    ["Page fill height"] = compact and 237 or 600}
local ordinary_heights = {["Natural height"] = 64, ["Short height"] = 128,
    ["Percent height"] = 64}
local pages_by_mode = compact and {2, 2, 2, 3} or {1, 1, 1, 2}
for mode=0,3 do
    local view=Cre.newDocView(width,height,1)
    local typeset=attach(view)
    typeset:setBlockRenderingMode(mode)
    configure(view)
    assert(view:loadDocument(source,false))
    view:renderDocument()
    assert(view:getPages() == pages_by_mode[mode+1])
    local marks = {}
    print(string.format("MODE %s pages=%d", modes[mode+1], view:getPages()))
    local frame=BB.new(width,height,BB.TYPE_BB8)
    for _,phrase in ipairs(passages) do
        local hits = assert(view:findAllText(phrase,false,false,8,true,0,255))
        local hit = assert(hits[1])
        assert(view:getTextFromXPointers(hit.start,hit["end"]) == phrase)
        local page=view:getPageFromXPointer(hit.start)
        local y=view:getPosFromXPointer(hit.start)
        marks[phrase] = {y=y, page=page, hit=hit}
        view:gotoPage(page)
        local box=assert(view:getWordBoxesFromPositions(hit.start,hit["end"],true)[1])
        local summary=string.format("MARK %s|%s|p=%d|x=%d|y=%d|doc_y=%d",
              modes[mode+1],phrase,page,box.x0,box.y0,y)
        if shades[phrase] then
            view:drawCurrentPage(frame,false,false,false,false)
            local want=shades[phrase]
            local at=box.y0+3
            local right=0
            for px=0,width-1 do
                if frame:getPixel(px,at):getColor8().a == want then right=px end
            end
            assert(right > 0)
            local top,bottom=at,at
            while top > 0 and frame:getPixel(right,top-1):getColor8().a == want do top=top-1 end
            while bottom+1<height and frame:getPixel(right,bottom+1):getColor8().a == want do bottom=bottom+1 end
            local expect = mode == 3 and web_heights[phrase] or nil
            assert(bottom+1-top == (expect or ordinary_heights[phrase] or 64))
            assert(right == (mode == 3 and phrase == "Web wide and tall" and 473 or 1209))
            marks[phrase].bottom = bottom+1
            summary=summary..string.format("|paint=%d..%d|h=%d|right=%d",top,bottom+1,bottom+1-top,right)
        end
        print(summary)
    end
    assert(marks["Tall follows"].y - marks["Tall height"].y == (mode == 3 and 164 or 64))
    assert(marks["Short continuation"].y - marks["Short height"].y == 64)
    assert(marks["Last line"].page == pages_by_mode[mode+1])
    if compact and mode == 3 then
        assert(marks["Page fill height"].bottom == height - 54)
        view:gotoPage(3)
        view:drawCurrentPage(frame, false, false, false, false)
        assert(frame:getPixel(1209, 54):getColor8().a == 184)
        assert(frame:getPixel(1209, 416):getColor8().a == 184)
        assert(frame:getPixel(1209, 417):getColor8().a ~= 184)
    end
    for page = 1, view:getPages() do
        view:gotoPage(page)
        view:drawCurrentPage(frame, false, false, false, false)
        frame:writePNG(captures .. "/" .. modes[mode+1] .. "-" .. page .. ".png", false)
    end
    if mode == 3 then
        local start, finish = marks["Last line"].hit.start, marks["Last line"].hit["end"]
        for _, next_mode in ipairs{2, 3} do
            typeset:setBlockRenderingMode(next_mode)
            view:renderDocument()
            assert(view:getTextFromXPointers(start, finish) == "Last line")
            view:gotoXPointer(start)
            assert(view:getPageFromXPointer(start) == pages_by_mode[next_mode+1])
        end
    end
    frame:free()
    view:close()
end
Device:exit()
