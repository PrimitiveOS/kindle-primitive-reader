-- Run from the pinned native runtime: specimen, CRE cache, Noto Sans font, output directory.
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
local function find(items, id)
    for _, item in ipairs(items) do
        if item.tweak_id == id then return item end
        if type(item.sub_item_table) == "table" then
            local result = find(item.sub_item_table, id)
            if result then return result end
        end
    end
end
local ids = {
    "table_full_width", "table_td_width_auto", "table_margin_left_right_auto",
    "table_no_presentational_hints",
}
local toggles = {}
for i, id in ipairs(ids) do
    toggles[i] = assert(find(menu.style_tweaks.sub_item_table, id))
    assert(toggles[i].enabled_func() and not toggles[i].checked_func())
end
local function choose(index, checked)
    local current = applied
    toggles[index].callback()
    assert(toggles[index].checked_func() == checked and applied == current + 1)
end

local phrases = { "Pier map", "Beacon", "North", "Low tide", "Bell", "East", "Horn",
    "South", "Ferry", "West", "Wharf", "Cedar", "Birch", "Willow", "Alder" }
local pointers
local function sample(mode, choice)
    local width, height, margin = 617, 900, 30
    local view = Cre.newDocView(width, height, 1)
    view:setIntProperty("crengine.render.block.rendering.flags", mode == "book" and 0x03375131 or 0x7fffffff)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setStyleSheet("./data/epub.css", controller:getCssText())
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(margin, margin, margin, margin)
    view:setFontFace("Noto Sans")
    view:setFontSize(30)
    assert(view:loadDocument(source, false))
    view:renderDocument()
    local anchors, x = {}, {}
    for i, phrase in ipairs(phrases) do
        local hits = assert(view:findAllText(phrase, false, false, 16, true, 0, 255))
        assert(#hits == 1, phrase)
        local hit = hits[1]
        assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase)
        anchors[i] = hit.start
        local _, px = view:getPosFromXPointer(hit.start)
        x[i] = px
        assert(view:getPageFromXPointer(hit.start) == view:getPageFromXPointer(anchors[1]))
    end
    local locations = table.concat(anchors, ",")
    pointers = pointers or locations
    assert(locations == pointers)
    view:gotoPage(view:getPageFromXPointer(anchors[1]))
    local pixels = BB.new(width, height, BB.TYPE_BB8)
    view:drawCurrentPage(pixels, false, false, false, false)
    pixels:writePNG(string.format("%s/stock-table-width-%s-%s.png", output, mode, choice), false)
    pixels:free()
    view:close()
    print(mode, choice, table.concat(x, ","))
    return x
end

for _, mode in ipairs({ "book", "web" }) do
    local base = sample(mode, "publisher")
    choose(1, true)
    local full = sample(mode, "full")
    choose(2, true)
    local full_auto = sample(mode, "full-auto")
    choose(1, false)
    local auto = sample(mode, "auto")
    choose(3, true)
    local auto_center = sample(mode, "auto-center")
    choose(2, false)
    local center = sample(mode, "center")
    choose(1, true)
    choose(2, true)
    local all = sample(mode, "all")
    for i = 1, 3 do choose(i, false) end
    local restored = sample(mode, "restored")
    choose(2, true)
    choose(4, true)
    local no_hints = sample(mode, "auto-no-html")
    choose(4, false)
    choose(2, false)
    assert(base[2] == full[2] and full[3] - base[3] > 300)
    assert(center[2] - base[2] == center[3] - base[3] and center[2] > 150)
    assert(base[5] == auto[5] and base[6] - auto[6] > 200)
    assert(auto_center[5] - auto[5] == auto_center[6] - auto[6] and auto_center[5] > 150)
    assert(full_auto[6] == full[6] and full_auto[8] < full[8] - 80)
    assert(base[10] == auto[10] and base[11] == auto[11])
    assert(base[15] > auto[15] + 50 and auto[15] > 160)
    assert(no_hints[15] == auto[15] and no_hints[13] < auto[13] - 20)
    assert(table.concat(full_auto, ",") == table.concat(all, ","))
    assert(table.concat(base, ",") == table.concat(restored, ","))
end
