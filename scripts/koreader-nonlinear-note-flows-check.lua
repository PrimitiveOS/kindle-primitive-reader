-- Run in the pinned stock runtime with a temporary KO_HOME and cache.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
G_reader_settings = require("luasettings"):wrap({})
G_reader_settings:saveSetting("style_tweaks", {})
require("document/canvascontext"):init(require("device"))
local Cre = require("libs/libkoreader-cre")
local CreDocument = require("document/credocument")
local Rolling = require("apps/reader/modules/readerrolling")
local StyleTweak = require("apps/reader/modules/readerstyletweak")
local Typeset = require("apps/reader/modules/readertypeset")
local BB = require("ffi/blitbuffer")
local source, cache, font, output = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
local width, height = 720, 480

local opts = require("luasettings"):wrap({})
local tweaker = StyleTweak:new{ui = {
    menu = {registerToMainMenu = function() end},
    doc_settings = opts,
    handleEvent = function(_, event) assert(event.handler == "onApplyStyleSheet") end,
}}
tweaker:onReadSettings(opts)
local menu = {}
tweaker:addToMainMenu(menu)
local function find(items, id)
    for _, item in ipairs(items) do
        if item.tweak_id == id then return item end
        if type(item.sub_item_table) == "table" then
            local found = find(item.sub_item_table, id)
            if found then return found end
        end
    end
end
for _, id in ipairs({"footnote-inpage_epub", "inpage_footnote_combine_non_linear"}) do
    local item = assert(find(menu.style_tweaks.sub_item_table, id))
    assert(item.enabled_func() and not item.checked_func(), id)
    item.callback()
    assert(item.checked_func())
end
local stylesheet = assert(tweaker:getCssText())
assert(stylesheet:find("non-linear-combining", 1, true))

Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)
local view = Cre.newDocView(width, height, 1)
local typeset = setmetatable({ui = {
    document = {setBlockRenderingFlags = function(_, flags)
        assert(flags == 0x03375131)
        view:setIntProperty("crengine.render.block.rendering.flags", flags)
    end},
    handleEvent = function(_, event) assert(event.handler == "onUpdatePos") end,
}}, {__index = Typeset})
typeset:setBlockRenderingMode(2)
view:setStyleSheet("./data/epub.css", stylesheet)
view:setVisiblePageCount(1, false)
view:setHeaderInfo(0)
view:setPageMargins(32, 32, 32, 32)
view:setFontSize(25)
view:setIntProperty("crengine.footnotes", 1)
view:setDefaultInterlineSpace(120)
view:setIntProperty("crengine.render.dpi", 96)
view:setFontFace("Noto Sans")
assert(view:loadDocument(source, false))
view:renderDocument()
local document = setmetatable({
    _document = view, _view_mode = CreDocument.PAGE_VIEW_MODE, hide_nonlinear_flows = false,
}, {__index = CreDocument})
local function find_position(phrase)
    local hits = assert(view:findAllText(phrase, false, false, 4, true, 0, 255))
    assert(#hits == 1, phrase)
    assert(view:getTextFromXPointers(hits[1].start, hits[1]["end"]) == phrase)
    return hits[1].start
end
local origin = find_position("The harbor opened")
document:cacheFlows()
assert(document:hasNonLinearFlows() and not document:hasHiddenFlows())
local regular_unforced = document:getPageCount()
local function update(_, force)
    assert(force)
    view:renderDocument()
    view:gotoPage(view:getPageFromXPointer(origin))
    document:cacheFlows()
end
local rolling = setmetatable({
    hide_nonlinear_flows = false, ui = {document = document}, onUpdatePos = update,
}, {__index = Rolling})
rolling:onToggleHideNonlinear()
assert(document:hasHiddenFlows())
local first = view:getPageFromXPointer(origin)
local function page(phrase) return view:getPageFromXPointer(find_position(phrase)) end
local a, b, c = page("Red lantern schedule"), page("Interval of four beats"), page("Lower channel notation")
local middle, final = page("The bridge remained clear"), page("We closed the field book")
local route = page("White water broke")
local d = page("Morning reading")
assert(document:getPageFlow(first) == 0)
local adjacent = document:getPageFlow(a)
assert(adjacent > 0 and document:getPageFlow(b) == adjacent)
assert(document:getPageFlow(c) > 0 and document:getPageFlow(c) ~= adjacent)
assert(document:getPageFlow(d) > 0 and document:getPageFlow(d) ~= adjacent)
assert(document:getNextPage(first) == middle and document:getNextPage(middle) == final)
assert(document:getPrevPage(middle) == first and document:getPrevPage(route) == final)
assert(document:getPageNumberInFlow(middle) == document:getPageNumberInFlow(first) + 1)
local all = document:getPageCount()
local linear = document:getTotalPagesInFlow(0)
assert(linear < all)
local function capture(page, name)
    view:gotoPage(page)
    local frame = BB.new(width, height, BB.TYPE_BB8)
    view:drawCurrentPage(frame, false, false, false, false)
    frame:writePNG(output .. "/" .. name .. ".png", false)
    frame:free()
end
capture(first, "stock-combined-note-band")
capture(a, "stock-combined-note-originals")
rolling:onToggleHideNonlinear()
assert(not document:hasHiddenFlows() and document:getPageCount() == regular_unforced)
assert(document:getPageFlow(view:getPageFromXPointer(origin)) == 0)
print(string.format("Stock combined footnote flows passed, %d regular of %d total pages", linear, all))
