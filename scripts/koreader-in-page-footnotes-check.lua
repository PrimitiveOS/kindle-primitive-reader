require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
require("document/canvascontext"):init(require("device"))
local Typeset = require("apps/reader/modules/readertypeset")
local StyleTweak = require("apps/reader/modules/readerstyletweak")
local Settings = require("luasettings")
local UIManager = require("ui/uimanager")
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")

local source, cache, font, captures = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
local width, height, margin = 720, 480, 32
local tweak_id = "footnote-inpage_epub"
local function find_tweak(items)
    for _, item in ipairs(items) do
        if item.tweak_id == tweak_id then return item end
        if type(item.sub_item_table) == "table" then
            local child = find_tweak(item.sub_item_table)
            if child then return child end
        end
    end
end

G_reader_settings:saveSetting("style_tweaks", {})
local settings = Settings:wrap({})
local applies = 0
local registered = {}
local controller = StyleTweak:new{ ui = {
    menu = { registerToMainMenu = function(_, widget) table.insert(registered, widget) end },
    doc_settings = settings,
    handleEvent = function(_, event)
        assert(event.handler == "onApplyStyleSheet")
        applies = applies + 1
    end,
} }
assert(registered[1] == controller)
controller:onReadSettings(settings)
local menu = {}
controller:addToMainMenu(menu)
local row = assert(find_tweak(menu.style_tweaks.sub_item_table))
local master = menu.style_tweaks.sub_item_table[1]
assert(row.enabled_func() and not row.checked_func())
row.callback()
local css = assert(controller:getCssText())
assert(row.checked_func() and applies == 1)
assert(css:find("footnote%-inpage"))
master.callback()
assert(not row.enabled_func() and not controller:getCssText())
master.callback()
assert(row.enabled_func() and row.checked_func())
row.callback()
assert(not row.checked_func() and controller:getCssText() == "")
local details
local show = UIManager.show
UIManager.show = function(_, widget) details = widget end
row.hold_callback({ updateItems = function() end })
UIManager.show = show
assert(details and not details.is_global_default)
details.toggle_global_default_callback()
assert(row.checked_func() and controller.global_tweaks[tweak_id])
row.callback()
assert(not row.checked_func() and controller.doc_tweaks[tweak_id] == false)
controller:onSaveSettings()
controller:onReadSettings(Settings:wrap({}))
assert(row.checked_func() and controller.doc_tweaks[tweak_id] == nil)
controller:onReadSettings(settings)
assert(not row.checked_func() and controller.doc_tweaks[tweak_id] == false)
print("Pinned Style tweaks menu: tap, master switch, hold/default, saved book override")

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
        if gray < 200 and frame:getPixel(margin + math.floor(width / 7), y):getColor8().a > 235
            and frame:getPixel(margin, y - 1):getColor8().a > 235
            and frame:getPixel(margin, y + 1):getColor8().a > 235 then
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
local interleaved_ref = position(view, "The tide gauge showed")
local interleaved_note = position(view, "Tide gauge note")
local interleaved_after = position(view, "The gauge register continues")
assert(interleaved_ref < interleaved_note and interleaved_note <= interleaved_after)
assert(capture(view, interleaved_ref, "stock-note-interleaved-band"))
assert(not capture(view, interleaved_note, "stock-note-interleaved-canonical"))
local illustrated_ref = position(view, "The harbor key explains")
local illustrated_legend = position(view, "The chart legend names")
assert(illustrated_ref <= illustrated_legend)
assert(capture(view, illustrated_ref, "stock-note-illustrated-band"))
if illustrated_legend ~= illustrated_ref then
    capture(view, illustrated_legend, "stock-note-illustrated-page")
end
local forced_ref = position(view, "The keeper marked a second")
local restart = position(view, "The harbor record restarts")
assert(forced_ref < restart)
assert(capture(view, forced_ref, "stock-note-before-hard-break"))
view:close()
local ordinary = open("")
local ordinary_reference = position(ordinary, "The east pier")
assert(not capture(ordinary, ordinary_reference, "stock-note-disabled"))
ordinary:close()
print("Pinned KOReader Book mode renders bottom EPUB note bands and continued source lines")
