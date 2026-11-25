-- Pinned native KOReader replay for the quotation fixtures.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local Settings = require("luasettings")
G_reader_settings = Settings:wrap({})
require("document/canvascontext"):init(require("device"))
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local source, cache, font, output = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4])
local spine = assert(arg[5])
local long_spine = arg[6]
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)

local state = Settings:wrap({})
local repaint = 0
local controller = require("apps/reader/modules/readerstyletweak"):new{
    ui = {
        menu = { registerToMainMenu = function() end },
        doc_settings = state,
        handleEvent = function(_, event)
            assert(event.handler == "onApplyStyleSheet")
            repaint = repaint + 1
        end,
    },
}
controller:onReadSettings(state)
local menu = {}
controller:addToMainMenu(menu)
local function find(items, id)
    for _, item in ipairs(items) do
        if item.tweak_id == id then return item end
        if type(item.sub_item_table) == "table" then
            local found = find(item.sub_item_table, id)
            if found then return found end
        end
    end
end
local toggle = assert(find(menu.style_tweaks.sub_item_table, "no_pseudo_element_before_after"))
local cases = {
    "English", "French", "German", "Swiss", "Mixed", "Subtag",
    "Tokens", "Bundle", "Display", "Visibility", "ExtraClose", "ExtraSilent",
    "Custom", "None", "CarryOpen", "CarryMid", "CarryEnd",
}
local joiner = "⁠"
local function symbols(open, inner_open, close, inner_close)
    local function before(value, display) return 'Before="' .. value .. joiner .. '" ~' .. display .. '/' end
    local function after(value) return 'After="' .. joiner .. value .. '" ~i/' end
    return {
        O = before(open, "i"), o = before(inner_open, "i"), C = after(close), c = after(inner_close),
        x = before(open, "X"), ["+"] = "Before ~i/", ["-"] = "After ~i/",
    }
end
local en = symbols("“", "‘", "”", "’")
local fr = symbols("«", "“", "»", "”")
local de = symbols("„", "‚", "“", "‘")
local swiss = symbols("«", "‹", "»", "›")
local groups = {
    English = "OoOCcC", French = "OocC", German = "OocC", Swiss = "OocC",
    Mixed = "OoOCcC", Subtag = "OocococC", Tokens = "O+OC-C", Display = "xxOC",
    Visibility = "Ooc-", ExtraClose = "coc+", ExtraSilent = "-oc+", Custom = "OocC",
    None = "OocC", CarryOpen = "O", CarryMid = "oc", CarryEnd = "COC",
}
local function compare_tokens(name, case, tokens)
    local palette = en
    local keys = groups[case]
    if name == "spine-wiki" then
        keys = ({ Start = "O", Across = "OC", Close = "c", Normal = "oc" })[case]
    elseif name == "spine-long" then
        keys = ({ LongTail = "oc", MatchingClose = "C", RestoredDepth = "OC" })[case]
    elseif name == "spine" then
        keys = ({ Start = "O", Across = "oc", Close = "C", Normal = "OC" })[case]
    elseif name == "pseudo-off" then
        keys = ""
    elseif name == "none" then
        if case == "English" or case == "French" or case == "German" then
            keys = ""
        elseif case == "Tokens" then
            keys = "O+-C"
        else
            return
        end
    elseif case == "Bundle" then
        assert(#tokens == 1 and tokens[1] == 'Before="[]" ~i/', case)
        return
    elseif name == "main-fr" then
        palette = fr
    elseif name == "main-de" then
        palette = de
    elseif name ~= "main-en" then
        palette = ({ French = fr, German = de, Swiss = swiss })[case] or en
        if case == "Subtag" then keys = "OOCOCocC" end
        if case == "Mixed" then
            local mixed = {en.O, fr.O, en.o, en.c, fr.C, en.C}
            assert(table.concat(mixed) == table.concat(tokens), name .. " " .. case)
            return
        end
    end
    assert(#keys == #tokens, name .. " " .. case)
    for i = 1, #keys do
        assert(palette[keys:sub(i, i)] == tokens[i], name .. " " .. case .. " " .. i)
    end
end

local canonical = {}
local first_frame = {}
local function compare(name, main, embedded, capture, stylesheet, fixture, phrases)
    local width, height = 860, 920
    local view = Cre.newDocView(width, height, 1)
    view:setIntProperty("crengine.render.block.rendering.flags", 0x03375131)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setStringProperty("crengine.textlang.main.lang", main)
    view:setStringProperty("crengine.textlang.embedded.langs.enabled", embedded and "1" or "0")
    if stylesheet == false then
        view:setStyleSheet(nil, controller:getCssText())
    else
        view:setStyleSheet(stylesheet or "./data/epub.css", controller:getCssText())
    end
    view:setPageMargins(40, 40, 40, 40)
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setFontFace("Noto Sans")
    view:setFontSize(22)
    assert(view:loadDocument(fixture or source, false))
    view:renderDocument()
    local first
    local html = assert(io.open(output .. "/" .. name .. ".html", "w"))
    for _, case in ipairs(phrases or cases) do
        local label = "Case " .. case
        local hits = assert(view:findAllText(label, false, true, 2, true, 0, 255), label)
        assert(#hits == 1, label)
        local xp = hits[1].start
        assert(view:getTextFromXPointers(xp, hits[1]["end"]) == label, label)
        canonical[case] = canonical[case] or xp
        assert(canonical[case] == xp, label)
        first = first or view:getPageFromXPointer(xp)
        if not fixture then assert(first == view:getPageFromXPointer(xp), label) end
        if case == "Across" then assert(first < view:getPageFromXPointer(xp)) end
        local details = assert(view:getHTMLFromXPointer(xp, 0x0C40, true), label)
        html:write(details, "\n")
        local tokens = {}
        for node in details:gmatch("<pseudoElem ([^>]+)") do
            table.insert(tokens, node)
            print("TOKEN", name, case, node)
        end
        compare_tokens(name, case, tokens)
    end
    assert(#(view:findAllText("“", false, true, 8, true, 0, 255) or {}) == 0)
    html:close()
    if capture then
        view:gotoPage(first)
        local page = BB.new(width, height, BB.TYPE_BB8)
        view:drawCurrentPage(page, false, false, false, false)
        local bytes = BB.tostring(page)
        if name == "restored" then
            assert(bytes == first_frame.publisher, "Restored frame differs")
        else
            first_frame[name] = bytes
        end
        page:writePNG(output .. "/stock-quote-marks-" .. name .. ".png", false)
        page:free()
    end
    view:close()
end

compare("publisher", "en", true, true)
compare("main-de", "de", false, true)
compare("main-fr", "fr", false, false)
compare("main-en", "en", false, false)
compare("html5", "en", true, false, "./data/html5.css")
compare("none", "en", true, false, false)
assert(not toggle.checked_func())
toggle.callback()
assert(toggle.checked_func() and repaint == 1)
compare("pseudo-off", "en", true, true)
toggle.callback()
assert(not toggle.checked_func() and repaint == 2)
compare("restored", "en", true, true)
local spine_labels = {"Start", "Across", "Close", "Normal"}
compare("spine", "en", true, false, nil, spine, spine_labels)

local wikipedia = assert(find(menu.style_tweaks.sub_item_table, "footnote-inpage_wikipedia"))
assert(not wikipedia.checked_func())
wikipedia.callback()
assert(wikipedia.checked_func() and repaint == 3)
compare("spine-wiki", "en", true, false, nil, spine, spine_labels)
wikipedia.callback()
assert(not wikipedia.checked_func() and repaint == 4)
if long_spine then
    compare("spine-long", "en", true, true, nil, long_spine,
        {"LongTail", "MatchingClose", "RestoredDepth"})
end
