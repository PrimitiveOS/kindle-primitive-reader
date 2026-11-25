-- Run in the pinned native runtime with the XHTML specimen and fixture fonts.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local Settings = require("luasettings")
G_reader_settings = Settings:wrap({})
require("document/canvascontext"):init(require("device"))
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local source, cache, output = assert(arg[1]), assert(arg[2]), assert(arg[3])
Cre.initCache(cache, 0, true, 1)
for i = 4, #arg do Cre.registerFont(arg[i]) end

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
local leaves = {}
local function visit(items)
    for _, choice in ipairs(items) do
        if choice.tweak_id then leaves[choice.tweak_id] = choice end
        if type(choice.sub_item_table) == "table" then visit(choice.sub_item_table) end
    end
end
visit(menu.style_tweaks.sub_item_table)

local phrases = { "Wide plain link", "Held nested link", "Named anchor only",
    "Quiet shore text", "Decorated parent link", "Destination stays." }
local starts
local function measure(page, a, b)
    local low, row_ink, checksum = 255, 0, 0
    local left, right = math.max(0, a.x0), math.min(page:getWidth(), b.x1)
    local top, bottom = math.max(0, a.y0), math.min(page:getHeight(), b.y1 + 4)
    for y = top, bottom - 1 do
        local row = 0
        for x = left, right - 1 do
            local ink = page:getPixel(x, y):getColor8().a
            low = math.min(low, ink)
            if ink < 190 then row = row + 1 end
            checksum = (checksum * 31 + ink) % 2147483647
        end
        row_ink = math.max(row, row_ink)
    end
    return { gray = low, underline = row_ink > (right - left) * 0.9,
        width = right - left, checksum = checksum }
end

local function sample(name)
    local width, height, margin = 650, 640, 28
    local view = Cre.newDocView(width, height, 1)
    view:setIntProperty("crengine.render.block.rendering.flags", 0x03375131)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setStyleSheet("./data/epub.css", controller:getCssText())
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(margin, margin, margin, margin)
    view:setFontFace("Noto Sans")
    view:setFontSize(40)
    assert(view:loadDocument(source, false))
    view:renderDocument()
    local cursor, results = {}, {}
    local page = BB.new(width, height, BB.TYPE_BB8)
    for index, phrase in ipairs(phrases) do
        local matches = assert(view:findAllText(phrase, false, false, 16, true, 0, 255))
        assert(#matches == 1)
        local hit = matches[1]
        assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase)
        cursor[index] = hit.start
        if index == 1 then
            view:gotoPage(view:getPageFromXPointer(hit.start))
            view:drawCurrentPage(page, false, false, false, false)
        end
        local boxes = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true))
        local first, last = boxes[1], boxes[#boxes]
        if index == 1 or index == 2 or index == 5 then
            assert(view:getLinkFromPosition(first.x0 + 5, first.y0 + 8) == "#target")
        elseif index == 3 or index == 4 then
            assert(view:getLinkFromPosition(first.x0 + 5, first.y0 + 8) == "")
        end
        results[index] = measure(page, first, last)
    end
    local pointer_list = table.concat(cursor, ",")
    starts = starts or pointer_list
    assert(pointer_list == starts)
    page:writePNG(output .. "/stock-link-styles-" .. name .. ".png", false)
    page:free()
    view:close()
    return results
end

local base = sample("default")
local pairs_to_test = {
    { "a_black", "a_blue" },
    { "a_bold", "a_not_bold" },
    { "a_italic", "a_not_italic" },
    { "a_underline", "a_not_underline" },
}
local measured = {}
for _, pair in ipairs(pairs_to_test) do
    for index, id in ipairs(pair) do
        local leaf = assert(leaves[id])
        local before = applied
        assert(not leaf.checked_func())
        leaf.callback()
        assert(leaf.checked_func() and applied == before + 1)
        if index == 2 then assert(not leaves[pair[1]].checked_func()) end
        local stats = sample(id)
        assert(stats[4].checksum == base[4].checksum)
        measured[id] = stats
    end
    leaves[pair[2]].callback()
    assert(not leaves[pair[2]].checked_func())
end

for _, index in ipairs({1, 2, 3, 5}) do
    assert(measured.a_black[index].gray == 0 and measured.a_blue[index].gray == 63)
end
assert(measured.a_bold[1].checksum ~= base[1].checksum)
assert(measured.a_not_bold[2].checksum ~= base[2].checksum)
assert(measured.a_italic[1].checksum ~= base[1].checksum)
assert(measured.a_not_italic[2].checksum ~= base[2].checksum)
assert(measured.a_underline[1].underline and not measured.a_underline[3].underline)
assert(base[5].underline and measured.a_not_underline[5].underline)
assert(not measured.a_not_underline[2].underline)
print("Four exclusive pairs; black 0, blue 63; nested and named anchors; parent underline retained; source and link hits retained")
