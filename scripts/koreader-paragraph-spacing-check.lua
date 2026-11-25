-- Run from the pinned native KOReader runtime with the authored XHTML fixture.
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

local choices = {
    none = "paragraph_no_whitespace",
    half = "paragraph_whitespace_half",
    full = "paragraph_whitespace",
    web = "paragraph_web_browser_style",
}
for name, id in pairs(choices) do
    choices[name] = assert(find(menu.style_tweaks.sub_item_table, id))
end
local function choose(name)
    local current = applied
    choices[name].callback()
    assert(choices[name].checked_func() and applied == current + 1)
end

local phrases = { "Morning rain.", "Water fills the bowl.", "Mist softens the harbor.",
    "Afternoon", "A late ferry returns.", "A bright signal." }
local pointers
local function sample(name)
    local width, height, margin, size = 617, 760, 30, 40
    local view = Cre.newDocView(width, height, 1)
    view:setIntProperty("crengine.render.block.rendering.flags", 0x03375131)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setStyleSheet("./data/epub.css", controller:getCssText())
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(margin, margin, margin, margin)
    view:setFontFace("Noto Sans")
    view:setFontSize(size)
    assert(view:loadDocument(source, false))
    view:renderDocument()
    local anchors, x, y = {}, {}, {}
    for i, phrase in ipairs(phrases) do
        local hits = assert(view:findAllText(phrase, false, false, 16, true, 0, 255))
        assert(#hits == 1, phrase)
        local hit = hits[1]
        assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase)
        anchors[i] = hit.start
        y[i], x[i] = view:getPosFromXPointer(hit.start)
    end
    assert(view:getPageFromXPointer(anchors[1]) == view:getPageFromXPointer(anchors[#anchors]))
    local locations = table.concat(anchors, ",")
    pointers = pointers or locations
    assert(locations == pointers)
    view:gotoPage(view:getPageFromXPointer(anchors[1]))
    local pixels = BB.new(width, height, BB.TYPE_BB8)
    view:drawCurrentPage(pixels, false, false, false, false)
    pixels:writePNG(output .. "/stock-paragraph-spacing-" .. name .. ".png", false)
    pixels:free()
    view:close()
    return { y[2] - y[1], y[3] - y[2], y[5] - y[4], y[6] - y[5], x[6] - x[5], x[5] }
end

local measured = {}
for _, name in ipairs({ "none", "half", "full", "web" }) do
    if name == "web" then
        choices.full.callback()
        assert(not choices.full.checked_func())
    end
    choose(name)
    if name == "half" then assert(not choices.none.checked_func()) end
    if name == "full" then assert(not choices.half.checked_func()) end
    measured[name] = sample(name)
    io.write(name, "  ", table.concat(measured[name], "  "), "\n")
end

for i = 1, 2 do
    assert(measured.half[i] - measured.none[i] == 20)
    assert(measured.full[i] - measured.none[i] == 40)
    assert(measured.web[i] - measured.none[i] == 40)
end
assert(measured.full[3] == measured.half[3])
assert(measured.web[4] - measured.none[4] == 80)
assert(measured.web[5] == 80 and measured.web[6] < measured.none[6])
