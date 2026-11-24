require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
local Device = require("device")
require("document/canvascontext"):init(Device)
local Typeset = require("apps/reader/modules/readertypeset")
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local bit = require("bit")
local source, cache, font, captures = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)
local width, height = 1264, 1606
local property = "crengine.render.block.rendering.flags"
local modes = { "Legacy", "Flat", "Book", "Web" }
local expected = { 0x00000000, 0x03030031, 0x03375131, 0x7FFFFFFF }
local passages = {
    "Before joined margins", "After joined margins", "Before negative margins", "After negative margins",
    "Before net negative gap", "After net negative gap",
    "Centered box marker", "Centered container marker", "Pixel box marker", "After pixel box marker",
    "Floating card marker", "Neighbor starts here", "Clearance marker",
    "Before inline cluster", "Inside inline cluster", "after inline cluster",
    "First table cell", "Second table cell", "End of specimen",
    "Before empty breaks", "After empty breaks", "After nonempty break",
}
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
local result = {}
for mode = 0, 3 do
    local view = Cre.newDocView(width, height, 1)
    local typeset, updates = attach(view)
    typeset:setBlockRenderingMode(mode)
    assert(view:getIntProperty(property) == expected[mode + 1])
    typeset:ensureSanerBlockRenderingFlags()
    local sanitised = view:getIntProperty(property)
    assert(sanitised == bit.band(expected[mode + 1], bit.bnot(0x03000000)))
    assert(updates() == 2)
    typeset.ensure_saner_block_rendering_flags = nil
    typeset:setBlockRenderingMode(mode)
    configure(view)
    assert(view:loadDocument(source,false))
    view:renderDocument()
    print(string.format("MODE %s flags=%08x old_dom=%08x pages=%d", modes[mode+1],
        view:getIntProperty(property), sanitised, view:getPages()))
    local markers = {}
    result[mode + 1] = markers
    for _, phrase in ipairs(passages) do
        local hits = view:findAllText(phrase,false,false,8,true,0,255) or {}
        local exact
        for _, hit in ipairs(hits) do
            if view:getTextFromXPointers(hit.start,hit["end"]) == phrase then
                exact = hit
                break
            end
        end
        local hit = exact or hits[1]
        if hit then
            local page = view:getPageFromXPointer(hit.start)
            view:gotoPage(page)
            local words = view:getWordBoxesFromPositions(hit.start,hit["end"],true) or {}
            local box = words[1]
            if box then
                local y, x = view:getPosFromXPointer(hit.start)
                markers[phrase] = { box = box, page = page, doc_y = y }
                print(string.format("MARK %s|%s|p=%d|x=%d|y=%d|doc_x=%d|doc_y=%d",
                    modes[mode+1],phrase,page,box.x0,box.y0,x,y))
            else
                print("NOBOX " .. modes[mode+1] .. "|" .. phrase)
            end
        else
            print("MISSING " .. modes[mode+1] .. "|" .. phrase)
        end
    end
    view:gotoPage(1)
    local frame = BB.new(width,height,BB.TYPE_BB8)
    view:drawCurrentPage(frame,false,false,false,false)
    for _, band in ipairs{
        {"Centered box marker", 230}, {"Centered container marker", 200}, {"Pixel box marker", 221}
    } do
        local mark = assert(markers[band[1]])
        assert(mark.page == 1)
        local y = mark.box.y0 + 3
        local left, right = width, 0
        for x = 0,width-1 do
            if frame:getPixel(x,y):getColor8().a == band[2] then
                left, right = math.min(left,x), math.max(right,x)
            end
        end
        assert(left <= right)
        mark.band_width = right - left + 1
        print(string.format("BAND %s|%s|x=%d|w=%d",modes[mode+1],band[1],left,mark.band_width))
    end
    frame:writePNG(captures .. "/" .. modes[mode+1] .. ".png", false)
    frame:free()
    view:close()
end
local legacy, flat, book, web = unpack(result)
local function pitch(mode, before, after)
    return mode[after].doc_y - mode[before].doc_y
end
for _, check in ipairs{{legacy, 184, 136, 71}, {flat, 144, 103, 64},
                       {book, 144, 103, 64}, {web, 144, 103, 46}} do
    local mode, positive, mixed, negative = unpack(check)
    assert(pitch(mode, "Before joined margins", "After joined margins") == positive)
    assert(pitch(mode, "Before negative margins", "After negative margins") == mixed)
    assert(pitch(mode, "Before net negative gap", "After net negative gap") == negative)
end
assert(flat["Neighbor starts here"].box.x0 == flat["Floating card marker"].box.x0)
assert(pitch(flat, "Floating card marker", "Neighbor starts here") > 0)
for _, mode in ipairs{book, web} do
    assert(mode["Neighbor starts here"].box.x0 - mode["Floating card marker"].box.x0 == 416)
    assert(pitch(mode, "Floating card marker", "Neighbor starts here") == 0)
end
assert(not legacy["First table cell"] and not legacy["Second table cell"])
for _, mode in ipairs{flat, book, web} do
    assert(mode["First table cell"].box.x0 < mode["Second table cell"].box.x0)
    assert(pitch(mode, "First table cell", "Second table cell") == 0)
end
assert(web["Pixel box marker"].band_width == 260)
assert(book["Pixel box marker"].band_width == 1156)
for _, check in ipairs{{legacy,3},{flat,1},{book,1},{web,3}} do
    local mode, expected = unpack(check)
    assert(mode["After empty breaks"].page - mode["Before empty breaks"].page == expected)
    assert(mode["After nonempty break"].page - mode["After empty breaks"].page == 1)
end

-- Change flags on the same document view and follow an existing source passage.
local live = Cre.newDocView(width, height, 1)
local typeset = attach(live)
typeset:setBlockRenderingMode(3)
configure(live)
assert(live:loadDocument(source, false))
live:renderDocument()
local anchor_text = "Neighbor starts here"
local hits = assert(live:findAllText(anchor_text, false, false, 8, true, 0, 255))
assert(#hits == 1)
local first, last = hits[1].start, hits[1]["end"]
local original_box
for _, mode in ipairs{3, 0, 1, 2, 3} do
    typeset:setBlockRenderingMode(mode)
    live:renderDocument()
    assert(live:getTextFromXPointers(first, last) == anchor_text)
    live:gotoXPointer(first)
    local box = assert(live:getWordBoxesFromPositions(first, last, true)[1])
    if mode == 3 then
        if original_box then
            assert(box.x0 == original_box.x0 and box.y0 == original_box.y0)
            assert(box.x1 == original_box.x1 and box.y1 == original_box.y1)
        else
            original_box = box
        end
    end
    print(string.format("RESUME %s x=%d y=%d", modes[mode + 1], box.x0, box.y0))
end
live:close()
Device:exit()
