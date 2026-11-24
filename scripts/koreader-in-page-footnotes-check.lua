require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
require("document/canvascontext"):init(require("device"))
local Typeset = require("apps/reader/modules/readertypeset")
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")

local source, cache, font, captures = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
local width, height, margin = 720, 480, 32
local function find_tweak(items)
    for _, item in ipairs(items) do
        if item.id == "footnote-inpage_epub" then return item.css end
        if type(item) == "table" then
            local child = find_tweak(item)
            if child then return child end
        end
    end
end
local css = assert(find_tweak(require("ui/data/css_tweaks")))
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)

local function open(tweak)
    local view = Cre.newDocView(width, height, 1)
    local typeset = setmetatable({ ui = {
        document = { setBlockRenderingFlags = function(_, flags)
            assert(flags == 0x03375131)
            view:setIntProperty("crengine.render.block.rendering.flags", flags)
        end },
        handleEvent = function(_, event) assert(event.handler == "onUpdatePos") end,
    }}, { __index = Typeset })
    typeset:setBlockRenderingMode(2)
    view:setStyleSheet("./data/epub.css", tweak)
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(margin, margin, margin, margin)
    view:setFontSize(25)
    view:setIntProperty("crengine.footnotes", 1)
    view:setDefaultInterlineSpace(120)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setFontFace("Noto Sans")
    assert(view:loadDocument(source, false))
    view:renderDocument()
    return view
end

local function position(view, phrase)
    local hits = assert(view:findAllText(phrase, false, false, 8, true, 0, 255))
    assert(#hits == 1, phrase)
    local hit = hits[1]
    assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase)
    local page = view:getPageFromXPointer(hit.start)
    view:gotoPage(page)
    local box = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true)[1])
    print(string.format("page=%d y=%d %s", page, box.y0, phrase))
    return page, box.y0
end

local function capture(view, page, name)
    view:gotoPage(page)
    local frame = BB.new(width, height, BB.TYPE_BB8)
    view:drawCurrentPage(frame, false, false, false, false)
    local separator
    for y = margin, height - margin do
        local gray = frame:getPixel(margin, y):getColor8().a
        if gray < 200 then
            local continuous = true
            for x = margin + 1, margin + math.floor(width / 7) - 1 do
                if frame:getPixel(x, y):getColor8().a ~= gray then
                    continuous = false
                    break
                end
            end
            if continuous then
                assert(not separator)
                separator = y
            end
        end
    end
    print(string.format("capture=%s page=%d separator=%s", name, page, tostring(separator)))
    frame:writePNG(captures .. "/" .. name .. ".png", false)
    frame:free()
    return separator
end

local view = open(css)
local reference = position(view, "The east pier")
local note = position(view, "The repair date")
assert(note > reference)
assert(capture(view, reference, "stock-note-band") == 360)
assert(not capture(view, note, "stock-note-canonical"))
local start = position(view, "The shoal appears")
local body = position(view, "The survey party")
assert(body > start + 2)
assert(capture(view, start, "stock-note-first-slice") == 120)
assert(capture(view, start + 1, "stock-note-only-slice") == 80)
assert(capture(view, body, "stock-note-last-slice") == 400)
local last = position(view, "Soundings end")
assert(last > body)
local previous = body
for i = 1, 17 do
    local page = position(view, string.format("Sounding %02d:", i))
    assert(page >= previous)
    previous = page
end
position(view, "Narrative resumes")
view:close()
local ordinary = open("")
local ordinary_reference = position(ordinary, "The east pier")
assert(not capture(ordinary, ordinary_reference, "stock-note-disabled"))
ordinary:close()
print("Pinned KOReader Book mode renders bottom EPUB note bands and continued source lines")
