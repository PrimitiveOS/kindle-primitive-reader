-- Run from the pinned native KOReader runtime with the authored image fixture.
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
local events = 0
local controller = require("apps/reader/modules/readerstyletweak"):new{
    ui = {
        menu = { registerToMainMenu = function() end },
        doc_settings = config,
        handleEvent = function(_, event)
            assert(event.handler == "onApplyStyleSheet")
            events = events + 1
        end,
    },
}
controller:onReadSettings(config)
local menu = {}
controller:addToMainMenu(menu)
local function find(items, id)
    for _, item in ipairs(items) do
        if item.tweak_id == id then return item end
        if type(item.sub_item_table) == "table" then
            local result = find(item.sub_item_table, id)
            if result then return result end
        end
    end
end
local full = assert(find(menu.style_tweaks.sub_item_table, "image_full_width"))
local middle = assert(find(menu.style_tweaks.sub_item_table, "image_valign_middle"))
assert(not full.checked_func() and not middle.checked_func())

local pointers
local function render(name)
    local width, height = 640, 880
    local view = Cre.newDocView(width, height, 1)
    view:setIntProperty("crengine.render.block.rendering.flags", 0x03375131)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setStyleSheet("./data/epub.css", controller:getCssText())
    view:setPageMargins(40, 40, 40, 40)
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setFontFace("Noto Sans")
    view:setFontSize(36)
    assert(view:loadDocument(source, false))
    view:renderDocument()
    local anchors, positions = {}, {}
    for index, phrase in ipairs({ "Lead text.", "Tail text.", "The lower dock." }) do
        local hits = assert(view:findAllText(phrase, false, false, 16, true, 0, 255))
        assert(#hits == 1 and view:getTextFromXPointers(hits[1].start, hits[1]["end"]) == phrase)
        anchors[index] = hits[1].start
        positions[index] = view:getPosFromXPointer(anchors[index])
    end
    local source_identity = table.concat(anchors, ";")
    pointers = pointers or source_identity
    assert(source_identity == pointers)
    view:gotoPage(view:getPageFromXPointer(anchors[1]))
    local pixels = BB.new(width, height, BB.TYPE_BB8)
    view:drawCurrentPage(pixels, false, false, false, false)
    pixels:writePNG(output .. "/stock-image-style-" .. name .. ".png", false)
    local frame = BB.tostring(pixels)
    pixels:free()
    view:close()
    return positions, frame
end
local base, original_frame = render("original")
assert(base[1] == base[2])
middle.callback()
assert(middle.checked_func() and not full.checked_func() and events == 1)
local centered, centered_frame = render("middle")
assert(centered[1] == centered[2] and original_frame ~= centered_frame)
full.callback()
assert(middle.checked_func() and full.checked_func() and events == 2)
local grouped = render("full-width")
assert(grouped[2] - grouped[1] > 84)
full.callback()
assert(middle.checked_func() and not full.checked_func() and events == 3)
print("Stock image presets preserve source pointers and independently change line alignment and image flow")
