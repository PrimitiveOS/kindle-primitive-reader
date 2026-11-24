require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
local Device = require("device")
require("document/canvascontext"):init(Device)
local Cre = require("libs/libkoreader-cre")
local Doc = require("document/credocument")
local ReaderFont = require("apps/reader/modules/readerfont")
local BB = require("ffi/blitbuffer")
local ffi = require("ffi")
require("ffi/freetype_h")
ffi.cdef("FT_Error FT_Done_FreeType(FT_Library library);")
local ft = ffi.load("./libs/libfreetype.6.dylib")
local source, cache, font, modern, captures = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4]), assert(arg[5])
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)
require("ui/widget/notification").notify = function() end

local library = ffi.new("FT_Library[1]")
assert(ft.FT_Init_FreeType(library) == 0)
local face = ffi.new("FT_Face[1]")
local sizes = {12, 16, 24, 25, 42, 48}
local cases = {
    {"AV", {-1, -2, -6, -6, -10, -12}, "Harbor"},
    {"VA", { 1,  1,  3,  3,   5,   6}, "Signal"},
    {"To", { 0, -1, -2, -2,  -3,  -3}, "Lantern"},
}
local function measure(path, classic)
    assert(ft.FT_New_Face(library[0], path, 0, face) == 0)
    local delta = ffi.new("FT_Vector[1]")
    for index, size in ipairs(sizes) do
        assert(ft.FT_Set_Pixel_Sizes(face[0], 0, size) == 0)
        for _, case in ipairs(cases) do
            local a = ft.FT_Get_Char_Index(face[0], case[1]:byte(1))
            local b = ft.FT_Get_Char_Index(face[0], case[1]:byte(2))
            assert(a ~= 0 and b ~= 0)
            assert(ft.FT_Get_Kerning(face[0], a, b, 0, delta) == 0)
            local expected = classic and case[2][index] or 0
            assert(delta[0].x == expected * 64, case[1] .. " at " .. size .. " got " .. tonumber(delta[0].x))
            assert(delta[0].y == 0)
            print(string.format("FT %s %d %s %d", classic and "classic" or "GPOS-only", size, case[1], expected))
        end
    end
    ft.FT_Done_Face(face[0])
end
measure(font, true)
measure(modern, false)
ft.FT_Done_FreeType(library[0])

local width, height = 1264, 1606
local view = Cre.newDocView(width, height, 1)
view:setStyleSheet("./data/epub.css", "")
view:setVisiblePageCount(1, false)
view:setHeaderInfo(0)
view:setPageMargins(54, 54, 54, 54)
view:setFontFace("Harbor Pair Test")
view:setFontSize(42)
view:setIntProperty("crengine.render.dpi", 96)
view:setIntProperty("font.hinting.mode", 0)
assert(view:loadDocument(source, false))
local bridge = setmetatable({_document = view}, {__index = Doc})
local changes = 0
local controller = setmetatable({
    configurable = {},
    ui = {
        document = bridge,
        handleEvent = function(_, event)
            assert(event.handler == "onUpdatePos")
            changes = changes + 1
        end,
    },
}, {__index = ReaderFont})

local function locate(text)
    local hit = assert((view:findAllText(text, false, false, 8, true, 0, 255) or {})[1])
    assert(view:getTextFromXPointers(hit.start, hit["end"]) == text)
    return hit
end
local function positions()
    local result = {}
    for index, case in ipairs(cases) do
        local hit = locate(case[3])
        local page = view:getPageFromXPointer(hit.start)
        assert(page == 1)
        local _, x = view:getPosFromXPointer(hit.start)
        result[index] = {x = x, hit = hit}
    end
    return result
end

assert(controller:onSetFontKerning(0))
view:renderDocument()
local off = positions()
assert(controller:onSetFontKerning(1))
view:renderDocument()
local fast = positions()
assert(changes == 2 and controller.configurable.font_kerning == 1)
for index, case in ipairs(cases) do
    local original, updated = off[index], fast[index]
    assert(updated.hit.start == original.hit.start)
    assert(updated.hit["end"] == original.hit["end"])
    local delta = updated.x - original.x
    assert(delta == case[2][5], case[1] .. " CREngine delta " .. delta)
    print(string.format("CRE %s Off %d Fast %d delta %d", case[1], original.x, updated.x, delta))
end
local frame = BB.new(width, height, BB.TYPE_BB8)
view:gotoPage(1)
view:drawCurrentPage(frame, false, false, false, false)
frame:writePNG(captures .. "/fast-kerning-stock.png", false)
frame:free()
view:close()
Device:exit()
