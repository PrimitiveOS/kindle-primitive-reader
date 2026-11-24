-- Run in the pinned KOReader Mac runtime with a scratch capture directory.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
G_reader_settings = require("luasettings"):wrap({})
local Device = require("device")
require("document/canvascontext"):init(Device)
local Screen = Device.screen
local UIManager = require("ui/uimanager")
local TouchMenu = require("ui/widget/touchmenu")
local InputContainer = require("ui/widget/container/inputcontainer")
local info = require("ui/elements/common_info_menu_table")
local page_turns = require("ui/elements/page_turns").sub_item_table

local changed = false
local opened = {}
local menu = setmetatable({
    screen_size = Screen:getSize(),
    tab_item_table = {
        {
            icon = "appbar.settings",
            {
                text = "Page turns",
                sub_item_table_func = function()
                    return {
                        { text = page_turns[1].text },
                        { text = page_turns[2].text, callback = function() changed = true end },
                        { text_func = function() return "Literal ±" end },
                    }
                end,
            },
            { text = "Skipped menu", ignored_by_menu_search = true },
        },
    },
    openMenu = function(_, path, animated)
        table.insert(opened, { path, animated })
    end,
}, { __index = TouchMenu })
assert(menu:search("with swipes")[1][3] == "1.1.2")
assert(menu:search("±")[1][1] == "Literal ±")
assert(#menu:search(".*") == 0 and #menu:search("skipped") == 0)

local background = InputContainer:new {
    onShowMenuSearch = function() return menu:onShowMenuSearch() end,
}
UIManager:show(background)
local function top() return UIManager._window_stack[#UIManager._window_stack].widget end
local function find_named(root, name)
    if type(root) ~= "table" then return end
    if root.title == name then return root end
    for i = 1, #root do
        local found = find_named(root[i], name)
        if found then return found end
    end
end

local function search()
    info.search_menu.callback()
    local dialog
    -- The stock search opens the InputDialog and its virtual keyboard.
    for _, item in ipairs(UIManager._window_stack) do
        if item.widget.title == "Search menu entry" then dialog = item.widget end
    end
    assert(dialog and dialog.buttons[1][2].text == "Search")
    dialog.getInputText = function() return "WiTh SwIpEs" end
    dialog.buttons[1][2].callback()
    assert(G_reader_settings:readSetting("menu_search_string") == "with swipes")
    local results = assert(find_named(menu.results_menu_container, "Search results"))
    assert(#results.item_table == 1 and results.item_table[1].text == "With swipes")
    return results.item_table[1]
end

local item = search()
item.callback()
local box = top()
assert(box.text:find("Page turns", 1, true) and box.text:find("With swipes", 1, true))
assert(box.ok_text == "Open" and #opened == 0 and not changed)
if arg[1] then
    UIManager:forceRePaint()
    Screen.bb:writePNG(arg[1] .. "/stock-menu-search-preview.png", false)
end
box.ok_callback()
assert(opened[1][1] == "1.1.2" and not opened[1][2] and not changed)

item = search()
item.hold_callback()
assert(opened[2][1] == "1.1.2" and not opened[2][2] and not changed)
print("Stock menu search verified: literal recursive names, remembered query, breadcrumb, and open/hold paths without executing the match")
