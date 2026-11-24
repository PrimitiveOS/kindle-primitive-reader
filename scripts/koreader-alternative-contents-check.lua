-- Run in the pinned Mac KOReader runtime with source, cache, font, and both authored EPUBs.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
local Device = require("device")
require("document/canvascontext"):init(Device)
local Cre = require("libs/libkoreader-cre")
local source, cache, font, headings, fragments = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4]), assert(arg[5])
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)

local file = assert(io.open(source, "r"))
local script = file:read("*a")
file:close()
local function segment(first, last)
    local at = assert(script:find(first, 1, true))
    return script:sub(at, assert(script:find(last, at + #first, true)) - 1)
end
local function open(path)
    local view = Cre.newDocView(1264, 1606, 1)
    view:setStyleSheet("./data/epub.css", "")
    view:setVisiblePageCount(1, false)
    view:setFontFace("Noto Sans")
    assert(view:loadDocument(path, false))
    view:renderDocument()
    return view
end

local popup, scheduled, event, shown, footer, closed, invalidated
local env = setmetatable({
    _ = function(text) return text end,
    ReaderToc = {},
    UIManager = {
        show = function(_, visual) popup = visual end,
        scheduleIn = function(_, seconds, run) assert(seconds == 0.5) scheduled = run end,
    },
    ConfirmBox = { new = function(_, options) return options end },
    Event = { new = function(_, kind) return kind end },
}, {__index = _G})
local function pinned(chunk)
    return setfenv(assert(loadstring(chunk)), env)()
end
local menu = pinned("return function(self, menu_items)\n" .. segment(
    "    if self.ui.document:canHaveAlternativeToc() then\n        menu_items.toc_alt_toc",
    "    -- Allow to have getTocTicksFlattened"
) .. "end")
pinned(segment("function ReaderToc:fillToc()", "function ReaderToc:validateAndFixToc()"))
pinned(segment("function ReaderToc:cleanUpTocTitle(", "function ReaderToc:onSetDimensions("))

local function comparison(path, expected)
    local view = open(path)
    local settings = {}
    local custom = false
    local reader = setmetatable({
        alt_toc_symbol = "#", toc_ticks_ignored_levels = {2},
        resetToc = function(self) self.toc = nil end,
        onShowToc = function() shown = true end,
        validateAndFixToc = function() end,
        view = {footer = {
            setTocMarkers = function(_, enabled) assert(enabled) end,
            onUpdateFooter = function() footer = true end,
        }},
        ui = {
            document = {
                canHaveAlternativeToc = function() return true end,
                isTocAlternativeToc = function() return view:isTocAlternativeToc() end,
                buildAlternativeToc = function() view:buildAlternativeToc() end,
                getToc = function() return view:getToc() end,
                invalidateCacheFile = function() invalidated = true end,
            },
            doc_settings = {
                isTrue = function(_, name) return settings[name] == true end,
                delSetting = function(_, name) settings[name] = nil end,
                makeTrue = function(_, name) settings[name] = true end,
            },
            handmade = {isHandmadeTocEnabled = function() return custom end},
            handleEvent = function(_, name) event = name end,
            reloadDocument = function() view:close() view = open(path) end,
        },
    }, {__index = env.ReaderToc})
    local items = {}
    menu(reader, items)
    local toggle = assert(items.toc_alt_toc)
    assert(toggle.enabled_func() and not toggle.checked_func())
    custom = true
    assert(not toggle.enabled_func())
    custom = false
    local menu_instance = {closeMenu = function() closed = true end}
    toggle.callback(menu_instance)
    assert(settings.alternative_toc == nil)
    popup.ok_callback()
    assert(settings.alternative_toc and closed and shown and footer)
    assert(event == "UpdateTopStatusBarMarkers" and #reader.toc_ticks_ignored_levels == 0)
    assert(toggle.checked_func())
    reader:fillToc()
    assert(#reader.toc == #expected)
    for index, entry in ipairs(reader.toc) do
        local title = reader:cleanUpTocTitle(entry.title, true)
        print(index, entry.depth, title, entry.xpointer)
        assert(entry.depth == expected[index][1] and title == expected[index][2])
    end
    if #expected == 4 then assert(reader.toc[2].xpointer == reader.toc[3].xpointer) end
    -- An ordinary reopen has no alternative cache here. fillToc rebuilds from its saved preference.
    view:close()
    view = open(path)
    assert(not toggle.checked_func())
    reader:resetToc()
    reader:fillToc()
    assert(toggle.checked_func() and #reader.toc == #expected)
    toggle.callback(menu_instance)
    popup.ok_callback()
    assert(settings.alternative_toc == nil and invalidated)
    assert(scheduled)
    scheduled()
    assert(not toggle.checked_func() and view:getToc()[1].title == "Publisher preface")
    view:close()
end

comparison(headings, {{1, "Harbor Survey"}, {2, "–"}, {3, "East softhyphen quay"}, {2, "West quay"}})
comparison(fragments, {{1, "–"}, {1, "–"}, {1, "–"}})
print("Pinned KOReader: per-book toggle, custom gate, heading depth, fragment fallback and restore passed")
Device:exit()
