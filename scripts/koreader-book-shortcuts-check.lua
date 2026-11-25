-- Run in the pinned KOReader Mac runtime. The scratch directory is an isolated KO_HOME parent.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
G_reader_settings = require("luasettings"):wrap({})
local Device = require("device")
require("document/canvascontext"):init(Device)
local lfs = require("libs/libkoreader-lfs")
local LuaSettings = require("luasettings")
local FileManager = require("apps/filemanager/filemanager")
local ReadHistory = require("readhistory")
local UIManager = require("ui/uimanager")
local Dispatcher = require("dispatcher")
local TouchMenu = require("ui/widget/touchmenu")
Dispatcher:init()

local scratch = assert(arg[1])
assert(os.getenv("KO_HOME") == scratch .. "/profile")
local captures = assert(arg[2])
local base = scratch .. "/Documents"
local folder = base .. "/Field atlas"
local nested = folder .. "/Charts"
for _, dir in ipairs({base, folder, nested}) do
    assert(lfs.mkdir(dir))
end
local a = folder .. "/Night landing.epub"
local b = nested .. "/Coastal dispatches.epub"
for _, path in ipairs({a, b}) do
    local file = assert(io.open(path, "w"))
    file:write("Test file for routing")
    file:close()
end

G_reader_settings:saveSetting("home_dir", base)
local Plugin = dofile("plugins/bookshortcuts.koplugin/main.lua")
local route, chooser, popup
local preference_path = scratch .. "/bookshortcuts.lua"
local plugin = setmetatable({
    settings_file = preference_path,
    ui = {
        folder_shortcuts = {
            getShortcutFullName = function() end,
            hasFolderShortcut = function() return false end,
        },
        file_chooser = {
            changeToPath = function(_, path) route = {"folder", path} end,
        },
    },
}, {__index = Plugin})
local original_open = FileManager.openFile
FileManager.instance = plugin.ui
FileManager.openFile = function(_, path) route = {"book", path} end
plugin:loadSettings()
assert(plugin.settings.directory_action == "FM")

local menu = {}
plugin:addToMainMenu(menu)
local getter = menu.book_shortcuts.sub_item_table_func
local touchmenu = { item_table = getter(), updateItems = function() end, page = 2 }
local show = UIManager.show
UIManager.show = function(_, widget) popup = widget end
touchmenu.item_table[1].callback(touchmenu)
chooser = assert(popup)
chooser.onConfirm(folder)
assert(plugin.shortcuts[folder] and touchmenu.page == 1)
touchmenu.item_table[1].callback(touchmenu)
chooser = assert(popup)
chooser.onConfirm(a)
assert(plugin.shortcuts[a])

local function row_for(path)
    local name = path == a and "Night landing.epub" or path
    for index, row in ipairs(getter()) do
        if row.text and row.text:find(name, 1, true) then
            row.idx = index
            return row
        end
    end
end

row_for(a).callback()
assert(route[1] == "book" and route[2] == a)
row_for(folder).callback()
assert(route[1] == "folder" and route[2] == folder)
local options = getter()[2].sub_item_table
assert(options[1].checked_func() and not options[3].enabled_func())
options[2].callback()
assert(options[2].checked_func() and options[3].enabled_func())
ReadHistory.hist = {{file = b}, {file = a}}
row_for(folder).callback()
assert(route[2] == a)
options[3].callback()
assert(options[3].checked_func())
row_for(folder).callback()
assert(route[2] == b)
plugin:onFlushSettings()
local saved = LuaSettings:open(preference_path)
assert(saved:isTrue(folder) and saved:isTrue(a))

touchmenu.item_table = getter()
local row = assert(row_for(a))
row.hold_callback(touchmenu, row)
local confirmation = assert(popup)
UIManager.show = show
confirmation.ok_callback()
assert(not plugin.shortcuts[a] and plugin.shortcuts[folder])
assert(lfs.attributes(a, "mode") == "file")
plugin:onFlushSettings()
assert(not LuaSettings:open(preference_path):isTrue(a))
local time = os.time
os.time = function() return 12 * 3600 end
local view = TouchMenu:new {
    width = Device.screen:getWidth(),
    tab_item_table = { { icon = "appbar.tools", unpack(getter()[2].sub_item_table) } },
}
view:switchMenuTab(1)
UIManager:show(view)
UIManager:forceRePaint()
Device.screen.bb:writePNG(captures .. "/stock-book-shortcut-folder-action.png", false)
UIManager:close(view)
os.time = time
FileManager.openFile = original_open
print("Stock book shortcut chooser, file/folder/last-read routing, recursion, preferences and removal verified")
