-- Run in the pinned native KOReader runtime with a scratch KO_HOME.
local scratch = assert(arg[1])
assert(os.getenv("KO_HOME") == scratch .. "/profile")
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local Settings = require("luasettings")
local source, destination = scratch .. "/Reading archive", scratch .. "/Saved editions"
G_reader_settings = Settings:wrap{ document_metadata_arc_folder = source }
local Device = require("device")
require("document/canvascontext"):init(Device)
local UI = require("ui/uimanager")
local lfs = require("libs/libkoreader-lfs")
local util = require("util")
local FileManager = require("apps/filemanager/filemanager")
FileManager.instance = {
    folder_shortcuts = {
        getShortcutFullName = function() end,
        hasFolderShortcut = function() return false end,
    },
}
assert(lfs.mkdir(source) and lfs.mkdir(destination))
assert(lfs.mkdir(source .. "/Nested"))

local function write(folder, file, value)
    assert(util.writeToFile(value, folder .. "/" .. file))
end

local function read(folder, file)
    return util.readFromFile(folder .. "/" .. file, "rb")
end

write(source, "first.lua", "return { notes = 'source only' }")
write(source, "shared.lua", "return { notes = 'earlier' }")
write(source, "shared.lua.old", "older backup")
write(source, "Separate notes.txt", "independent")
write(source .. "/Nested", "nested.lua", "return { notes = 'nested' }")
write(destination, "second.lua", "return { notes = 'destination only' }")
write(destination, "shared.lua", "return { notes = 'later' }")
local time = os.time()
assert(lfs.touch(source .. "/shared.lua", time - 20, time - 20))
assert(lfs.touch(destination .. "/shared.lua", time - 10, time - 10))

local menu = require("ui/elements/common_settings_menu_table")
local location = menu.document_metadata_arc.sub_item_table[2]
local updated = 0
local function shown()
    return assert(UI._window_stack[#UI._window_stack]).widget
end
location.callback{ updateItems = function() updated = updated + 1 end }
shown().buttons[1][1].callback() -- Choose folder
local chooser = shown()
assert(chooser:onMenuSelect{ path = scratch })
assert(chooser:onMenuSelect{ path = destination })
assert(chooser.path == destination)
assert(chooser:onMenuHold{ path = destination .. "/." })
local confirmation = shown()
assert(confirmation.buttons[1][1].text == "Cancel")
assert(confirmation.buttons[1][2].text == "Choose")
if arg[2] then
    UI:forceRePaint()
    Device.screen.bb:writePNG(arg[2] .. "/stock-archive-folder-move.png", false)
end
confirmation.buttons[1][1].callback()
assert(shown() == chooser and updated == 0)
assert(G_reader_settings:readSetting("document_metadata_arc_folder") == source)
assert(read(source, "shared.lua") == "return { notes = 'earlier' }")
assert(read(destination, "shared.lua") == "return { notes = 'later' }")

assert(chooser:onMenuHold{ path = destination .. "/." })
shown().buttons[1][2].callback()
assert(updated == 1)
assert(G_reader_settings:readSetting("document_metadata_arc_folder") == destination)
assert(not lfs.attributes(source .. "/first.lua"))
assert(not lfs.attributes(source .. "/shared.lua"))
assert(not lfs.attributes(source .. "/shared.lua.old"))
assert(read(source, "Separate notes.txt") == "independent")
assert(read(source .. "/Nested", "nested.lua") == "return { notes = 'nested' }")
assert(read(destination, "first.lua") == "return { notes = 'source only' }")
assert(read(destination, "second.lua") == "return { notes = 'destination only' }")
assert(read(destination, "shared.lua") == "return { notes = 'earlier' }")
print("Stock archive folder cancellation, relocation, destination merge and same-name replacement passed")
