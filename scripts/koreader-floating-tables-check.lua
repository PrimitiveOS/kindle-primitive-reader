-- Replay the native table cases through KOReader's render mode and style controls.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local Settings = require("luasettings")
G_reader_settings = Settings:wrap({})
require("document/canvascontext"):init(require("device"))
local Typeset = require("apps/reader/modules/readertypeset")
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")

local fixture, cache, font, output = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
local width, height = 640, 520
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)

local state = Settings:wrap({})
local controller = require("apps/reader/modules/readerstyletweak"):new{
    ui = {
        menu = { registerToMainMenu = function() end },
        doc_settings = state,
        handleEvent = function(_, event) assert(event.handler == "onApplyStyleSheet") end,
    },
}
controller:onReadSettings(state)
local menu = {}
controller:addToMainMenu(menu)
local function find(items)
    for _, item in ipairs(items) do
        if item.tweak_id == "table_no_presentational_hints" then return item end
        if type(item.sub_item_table) == "table" then
            local found = find(item.sub_item_table)
            if found then return found end
        end
    end
end
local hints = assert(find(menu.style_tweaks.sub_item_table))

local cases = {
    {"Left caption", "Port cell", "Below port", "Beside port", "After port"},
    {"Right note", "Quay", "Below quay", "Beside quay", "After quay"},
    {"RTL caption", "First cell", "Next cell", "Below RTL", "Beside RTL", "After RTL"},
    {"Inline caption", "Inline cell", "Inline lower", "Below inline", "After inline"},
    {"Lead caption", "Lead cell", "Lead lower", "Trail caption", "Trail cell", "Trail lower",
        "Along leading float", "After right clear"},
    {"Full caption", "Wide cell", "Wide lower", "Below wide float", "After wide float"},
    {"Centered caption", "Center cell", "Center lower", "Below centered", "After centered"},
}
local masks = { 0x00000000, 0x03030031, 0x03375131, 0x7FFFFFFF }
local modes = { "legacy", "flat", "book", "web" }
local canonical = {}
local baseline = {}
local book_geometry = {
    {"46,55,55,240,39", "119,54,84,40,150"},
    {"481,490,490,40,39", "48,83,113,40,150"},
    {"406,507,415,415,40,39", "48,83,83,113,40,150"},
    {"46,55,55,40,39", "48,83,113,150,180"},
    {"46,55,55,406,414,414,240,39", "48,83,113,158,193,223,40,260"},
    {"46,54,54,40,39", "48,83,113,150,180"},
}

local function check_geometry(ordinal, index, no_hints, x, y)
    if ordinal >= 3 and not no_hints and book_geometry[index] then
        local expected = book_geometry[index]
        assert(table.concat(x, ",") == expected[1] and table.concat(y, ",") == expected[2], index)
    else
        local neighbor = #y - 1
        local last_cell = neighbor - 1
        if index == 1 then last_cell = 1 end
        assert(y[neighbor + 1] > y[last_cell], index)
        if ordinal < 3 or (index == 1 or index == 2) and no_hints then
            assert(y[neighbor] > y[last_cell], index)
        end
        if index == 3 and ordinal >= 3 then
            assert(x[1] > 320 and x[2] > x[3] and x[neighbor] == 40 and y[neighbor] < y[2])
        end
        if index == 5 and ordinal >= 3 then
            assert(y[4] > y[3] and y[7] > y[3] and y[8] > y[6])
        end
    end
end

local function compare(ordinal, no_hints)
    assert(hints.checked_func() == no_hints)
    local mode = modes[ordinal]
    local view = Cre.newDocView(width, height, 1)
    local typeset = setmetatable({ ui = {
        document = { setBlockRenderingFlags = function(_, flags)
            assert(flags == masks[ordinal])
            view:setIntProperty("crengine.render.block.rendering.flags", flags)
        end },
        handleEvent = function(_, event) assert(event.handler == "onUpdatePos") end,
    }}, { __index = Typeset })
    typeset:setBlockRenderingMode(ordinal - 1)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setStyleSheet("./data/epub.css", controller:getCssText())
    view:setPageMargins(40, 40, 40, 40)
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setFontSize(22)
    view:setFontFace("Noto Sans")
    assert(view:loadDocument(fixture, false))
    view:renderDocument()
    for index, phrases in ipairs(cases) do
        local x, y, page = {}, {}, nil
        for i, phrase in ipairs(phrases) do
            local hits = assert(view:findAllText(phrase, false, true, 10, true, 0, 255))
            assert(#hits == 1, phrase)
            local hit = hits[1]
            assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase, phrase)
            canonical[phrase] = canonical[phrase] or hit.start
            assert(canonical[phrase] == hit.start, phrase)
            page = page or view:getPageFromXPointer(hit.start)
            assert(page == view:getPageFromXPointer(hit.start), phrase)
            view:gotoPage(page)
            local marker = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true))[1]
            x[i], y[i] = marker.x0, marker.y0
        end
        print("TABLE", mode, no_hints and "off" or "on", index, table.concat(x, ","), table.concat(y, ","))
        check_geometry(ordinal, index, no_hints, x, y)
        if (index == 1 or index == 3) and ordinal == 3 then
            local capture = index == 1 and "left" or "rtl"
            local frame = BB.new(width, height, BB.TYPE_BB8)
            view:drawCurrentPage(frame, false, false, false, false)
            local pixels = BB.tostring(frame)
            if no_hints then
                assert((pixels ~= baseline[index]) == (index == 1))
            elseif baseline[index] then
                assert(baseline[index] == pixels)
            else
                baseline[index] = pixels
            end
            frame:writePNG(output .. "/stock-floating-table-" .. capture
                .. (no_hints and "-without-hints" or "") .. ".png", false)
            frame:free()
        end
    end
    view:close()
end
for ordinal in ipairs(modes) do compare(ordinal, false) end
hints.callback()
compare(3, true)
hints.callback()
compare(3, false)
