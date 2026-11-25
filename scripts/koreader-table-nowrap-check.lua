-- Run from the pinned KOReader runtime with the chapter, cache, font, and output directory.
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
        if item.tweak_id == "table_no_presentational_hints" then return item end
        if type(item.sub_item_table) == "table" then
            local result = find(item.sub_item_table)
            if result then return result end
        end
    end
end
local choice = assert(find(menu.style_tweaks.sub_item_table))
assert(choice.enabled_func() and not choice.checked_func())

local source_locations
local function render(state)
    local width, height, margin = 617, 1020, 30
    local view = Cre.newDocView(width, height, 1)
    view:setIntProperty("crengine.render.block.rendering.flags", 0x7fffffff)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setStyleSheet("./data/epub.css", controller:getCssText())
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(margin, margin, margin, margin)
    view:setFontFace("Noto Sans")
    view:setFontSize(30)
    assert(view:loadDocument(source, false))
    view:renderDocument()

    local point, locations, first_page = {}, {}, nil
    for i, phrase in ipairs({ "Harbor", "beam", "Early", "ferry", "West", "signal",
                             "First watch", "Second shift", "Storm", "watch.", "South", "searchlight" }) do
        local hits = assert(view:findAllText(phrase, true, false, 16, true, 0, 255))
        -- The page heading also contains Harbor. Select the instrument cell.
        if i == 1 then assert(#hits == 2); table.remove(hits, 1) end
        assert(#hits == 1, phrase)
        local hit = hits[1]
        assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase)
        local page = view:getPageFromXPointer(hit.start)
        first_page = first_page or page
        assert(page == first_page, phrase)
        locations[i] = hit.start
        point[i] = view:getPosFromXPointer(hit.start)
    end
    locations = table.concat(locations, ",")
    source_locations = source_locations or locations
    assert(source_locations == locations)
    view:gotoPage(first_page)
    local pixels = BB.new(width, height, BB.TYPE_BB8)
    view:drawCurrentPage(pixels, false, false, false, false)
    pixels:writePNG(output .. "/stock-table-nowrap-" .. state .. ".png", false)
    pixels:free()
    view:close()
    print(state, table.concat(point, ","))
    return point
end

local publisher = render("publisher")
choice.callback()
assert(choice.checked_func() and applied == 1)
local filtered = render("without-html-hints")
choice.callback()
assert(not choice.checked_func() and applied == 2)
local restored = render("restored")

assert(publisher[1] == publisher[2] and filtered[1] < filtered[2])
for _, sample in ipairs({publisher, filtered}) do
    assert(sample[3] == sample[4] and sample[5] < sample[6])
    assert(sample[7] < sample[8] and sample[9] < sample[10])
    assert(sample[11] < sample[12])
end
assert(table.concat(publisher, ",") == table.concat(restored, ","))
