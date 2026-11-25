-- Run in native KOReader with the chapter, CRE cache, font, and output path.
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
local applies = 0
local controller = require("apps/reader/modules/readerstyletweak"):new{ ui = {
    menu = { registerToMainMenu = function() end },
    doc_settings = config,
    handleEvent = function(_, event)
        assert(event.handler == "onApplyStyleSheet")
        applies = applies + 1
    end,
} }
controller:onReadSettings(config)
local menu = {}
controller:addToMainMenu(menu)
local function find(items, id)
    for _, item in ipairs(items) do
        if item.tweak_id == id then return item end
        if type(item.sub_item_table) == "table" then
            local match = find(item.sub_item_table, id)
            if match then return match end
        end
    end
end
local bullets = assert(find(menu.style_tweaks.sub_item_table, "ul_li_type_disc"))
local decimal = assert(find(menu.style_tweaks.sub_item_table, "ol_li_type_decimal"))
assert(bullets.enabled_func() and decimal.enabled_func())
assert(not bullets.checked_func() and not decimal.checked_func())

local original_anchors
local function render(state)
    local width, height, margin = 617, 720, 30
    local view = Cre.newDocView(width, height, 1)
    view:setIntProperty("crengine.render.block.rendering.flags", 0x03375131)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setStyleSheet("./data/epub.css", controller:getCssText())
    view:setPageMargins(margin, margin, margin, margin)
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setFontFace("Noto Sans")
    view:setFontSize(30)
    assert(view:loadDocument(source, false))
    view:renderDocument()
    local markers, anchors, boxes = {}, {}, {}
    local phrases = { "Ring lantern", "Square flag", "Quiet bell", "Evening harbor",
                      "Inner watch", "Coast watch", "Harbor patrol" }
    for i, phrase in ipairs(phrases) do
        local hits = assert(view:findAllText(phrase, false, false, 16, true, 0, 255))
        assert(#hits == 1 and view:getPageFromXPointer(hits[1].start) == 1, phrase)
        local hit = hits[1]
        assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase, phrase)
        boxes[i] = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true))[1]
        anchors[i] = hit.start
    end
    local identity = table.concat(anchors, "|")
    original_anchors = original_anchors or identity
    assert(identity == original_anchors)
    for _, phrase in ipairs({ "7. Inner watch", "13. Harbor patrol" }) do
        assert(#(view:findAllText(phrase, false, false, 16, true, 0, 255) or {}) == 0, phrase)
    end

    view:gotoPage(1)
    local pixels = BB.new(width, height, BB.TYPE_BB8)
    view:drawCurrentPage(pixels, false, false, false, false)
    for i, rect in ipairs(boxes) do
        local result, ink = {}, 0
        for y = rect.y0, rect.y1 - 1 do
            for x = rect.x0 - 68, rect.x0 - 1 do
                local gray = x >= 0 and pixels:getPixel(x, y):getColor8().a or 255
                if gray < 192 then ink = ink + 1 end
                result[#result + 1] = string.char(gray)
            end
        end
        markers[i] = table.concat(result)
        print(state, phrases[i], rect.x0, rect.y0, ink)
        if i == 3 then assert((ink > 0) == bullets.checked_func(), "Quiet bell marker") end
    end
    pixels:writePNG(output .. "/stock-list-marker-" .. state .. ".png", false)
    pixels:free()
    view:close()
    return markers
end

local original = render("publisher")
bullets.callback()
assert(bullets.checked_func() and not decimal.checked_func())
local discs = render("bullets")
decimal.callback()
assert(bullets.checked_func() and decimal.checked_func())
local combined = render("forced")
bullets.callback()
assert(not bullets.checked_func() and decimal.checked_func())
local numbers = render("decimal")
decimal.callback()
assert(not bullets.checked_func() and not decimal.checked_func() and applies == 4)
local restored = render("restored")
for i = 1, #original do
    assert(original[i] == restored[i])
    if i <= 4 then
        assert(original[i] == numbers[i] and discs[i] == combined[i] and original[i] ~= discs[i])
    else
        assert(original[i] == discs[i] and numbers[i] == combined[i] and original[i] ~= numbers[i])
    end
end
