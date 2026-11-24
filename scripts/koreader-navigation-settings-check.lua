-- Run in Lua 5.1 with the pinned settings, ReaderLink and SkimToWidget sources.
local function read(path)
    local handle = assert(io.open(path))
    local source = handle:read("*a")
    handle:close()
    return source
end

local function block(source, first, last)
    local start = assert(source:find(first, 1, true))
    return source:sub(start, assert(source:find(last, start, true)) - 1)
end

local menu_source, link_source, skim_path = read(assert(arg[1])), read(assert(arg[2])), assert(arg[3])
local saved = {}
G_reader_settings = {
    readSetting = function(_, key, fallback)
        if saved[key] == nil then return fallback end
        return saved[key]
    end,
    saveSetting = function(_, key, value) saved[key] = value end,
    isTrue = function(_, key) return saved[key] == true end,
    flipNilOrFalse = function(_, key) saved[key] = not saved[key] end,
}
_ = function(text) return text end
T = function(text, value) return (text:gsub("%%1", tostring(value))) end
common_settings = {}
local generator = block(menu_source, "local function genGenericMenuEntry", "common_settings.back_to_exit =")
local options = block(menu_source, "common_settings.opening_page_location_stack =", "-- Auto-save settings")
assert(loadstring(generator .. options))()

local opening = common_settings.opening_page_location_stack
local ready_callback, opening_marks = nil, {}
local link = {
    ui = { registerPostReaderReadyCallback = function(_, callback) ready_callback = callback end },
    current = 0,
    addCurrentLocationToStack = function(self) opening_marks[#opening_marks + 1] = self.current end,
}
local register = assert(loadstring("return function(self)\n" .. block(
    link_source, 'if G_reader_settings:isTrue("opening_page_location_stack") then',
    "-- For relative local file links") .. "\nend"))()
assert(not opening.checked_func())
register(link)
assert(not ready_callback)
opening.callback()
assert(opening.checked_func())
register(link)
assert(ready_callback and #opening_marks == 0)
link.current = 42
ready_callback()
assert(opening_marks[1] == 42)
opening.callback()
assert(not opening.checked_func())

local function module(name, implementation)
    package.preload[name] = function() return implementation end
end
local widget = { new = function(_, value) return value end }
for _, name in ipairs({"button", "geometry", "gesturerange", "widget/horizontalgroup",
        "widget/horizontalspan", "widget/progresswidget", "widget/verticalgroup",
        "widget/verticalspan", "widget/container/framecontainer",
        "widget/container/movablecontainer", "widget/container/widgetcontainer"}) do
    local path = name == "button" and "widget/button" or name
    module("ui/" .. path, widget)
end
module("ui/widget/focusmanager", { extend = function(_, value) return value end })
module("ui/bidi", { mirroredUILayout = function() return false end })
module("ffi/blitbuffer", {})
module("ui/event", { new = function(_, name, value) return {name, value} end })
module("ui/uimanager", {})
module("ui/size", {
    border = {window = 6}, span = {horizontal_small = 4},
    padding = {fullscreen = 24, default = 12}, item = {height_big = 42},
    radius = {window = 28}, line = {medium = 2},
})
module("device", {
    screen = {
        getWidth = function() return 1264 end, getHeight = function() return 1680 end,
        scaleBySize = function(_, size) return size end,
    },
    hasKeys = function() return false end, hasDPad = function() return false end,
    hasKeyboard = function() return false end, isTouchDevice = function() return true end,
})
module("optmath", {})
module("util", {})
module("gettext", _)

local skim_type = assert(loadfile(skim_path))()
local skim_menu = common_settings.skim_dialog_position
assert(skim_menu.sub_item_table[2].checked_func())
local ui = {
    view = { shouldInvertBiDiLayoutMirroring = function() return false end },
    toc = { getTocTicksFlattened = function() return {4, 16} end },
    getCurrentPage = function() return 7 end,
    document = { getPageCount = function() return 32 end },
}
local function find_button(group, label)
    for _, child in ipairs(group) do
        if child.text == label then return child end
        local found = find_button(child, label)
        if found then return found end
    end
end
for index, expected in ipairs({"top", "center", "bottom"}) do
    local choice = skim_menu.sub_item_table[index]
    assert(choice.radio)
    choice.callback()
    assert(choice.checked_func() and skim_menu.text_func():find(expected, 1, true))
    local skim = setmetatable({ui = ui, ges_events = {}}, {__index = skim_type})
    skim:init()
    assert(skim[1].align == expected and skim[1].dimen.h == 1680)
    assert(skim.curr_page == 7 and skim.page_count == 32 and skim.progress_bar.percentage == 7/32)
    local frame_rows = skim.skimto_frame[1]
    local first_size, last_size = #(frame_rows[1] or {}), #(frame_rows[5] or {})
    if expected == "center" then
        assert(saved.skim_dialog_position == nil and first_size > 0 and last_size > 0)
    elseif expected == "top" then
        assert(first_size == 0 and last_size == 21)
    else
        assert(first_size == 21 and last_size == 0)
    end
    local requested
    skim.goToPage = function(_, page) requested = page end
    assert(find_button(frame_rows, "+10")).callback()
    assert(requested == 17)
    assert(find_button(frame_rows, "-10")).callback()
    assert(requested == -3)
end
print("Pinned opening-page callback, Skim placements and ten-page shortcuts passed")
