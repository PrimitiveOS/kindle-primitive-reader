-- Run in native KOReader with an authored chapter, CRE cache, font, and output directory.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local Settings = require("luasettings")
G_reader_settings = Settings:wrap({})
require("document/canvascontext"):init(require("device"))
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local source, cache, font, output = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)

local config = Settings:wrap({})
local applied = 0
local controller = require("apps/reader/modules/readerstyletweak"):new{ ui = {
    menu = { registerToMainMenu = function() end },
    doc_settings = config,
    handleEvent = function(_, event)
        assert(event.handler == "onApplyStyleSheet")
        applied = applied + 1
    end,
} }
controller:onReadSettings(config)
local menu = {}
controller:addToMainMenu(menu)
local function find(items)
    for _, item in ipairs(items) do
        if item.tweak_id == "no_pseudo_element_before_after" then return item end
        if type(item.sub_item_table) == "table" then
            local result = find(item.sub_item_table)
            if result then return result end
        end
    end
end
local choice = assert(find(menu.style_tweaks.sub_item_table))
assert(choice.enabled_func() and not choice.checked_func())

local original_pointers
local function render(state)
    local width, height, margin = 617, 780, 30
    local view = Cre.newDocView(width, height, 1)
    view:setIntProperty("crengine.render.block.rendering.flags", 0x03375131)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setStyleSheet("./data/epub.css", controller:getCssText())
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(margin, margin, margin, margin)
    view:setFontFace("Noto Sans")
    view:setFontSize(30)
    assert(view:loadDocument(source, false))
    view:renderDocument()

    local points, pointers, page = {}, {}, nil
    for i, phrase in ipairs({ "Alpha", "amber", "follows", "pilings", "beacon" }) do
        local hits = assert(view:findAllText(phrase, false, false, 16, true, 0, 255))
        assert(#hits == 1, phrase)
        local hit = hits[1]
        assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase)
        page = page or view:getPageFromXPointer(hit.start)
        assert(page == view:getPageFromXPointer(hit.start), phrase)
        local y, x = view:getPosFromXPointer(hit.start)
        points[i] = { x, y }
        pointers[i] = hit.start
    end
    local original = view:getTextFromXPointers(pointers[1], pointers[4])
    assert(original:gsub("%s+", " ") == "Alpha tide amber follows the ", original)
    local decorated = view:findAllText("Chart seven", false, false, 16, true, 0, 255) or {}
    assert(#decorated == 0)
    original_pointers = original_pointers or table.concat(pointers, ",")
    assert(original_pointers == table.concat(pointers, ","))

    view:gotoPage(page)
    local pixels = BB.new(width, height, BB.TYPE_BB8)
    view:drawCurrentPage(pixels, false, false, false, false)
    pixels:writePNG(output .. "/stock-pseudo-generated-" .. state .. ".png", false)
    pixels:free()
    view:close()
    for i, point in ipairs(points) do print(state, i, unpack(point)) end
    return points
end

local publisher = render("publisher")
choice.callback()
assert(choice.checked_func() and applied == 1)
local off = render("off")
choice.callback()
assert(not choice.checked_func() and applied == 2)
local restored = render("restored")
assert(publisher[1][1] > off[1][1] + 100)
for i, point in ipairs(publisher) do
    assert(point[1] == restored[i][1] and point[2] == restored[i][2])
end
