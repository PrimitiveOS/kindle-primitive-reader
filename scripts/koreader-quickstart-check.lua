-- Run in the pinned native Mac runtime with KO_HOME set to a scratch directory.
assert(os.getenv("KO_HOME"), "Supply a scratch KO_HOME")
require("setupkoenv")
G_defaults = require("luadefaults"):open()
G_reader_settings = require("luasettings"):wrap({})
local Device = require("device")
require("document/canvascontext"):init(Device)

local DataStorage = require("datastorage")
local QuickStart = require("ui/quickstart")
local ReaderUI = require("apps/reader/readerui")
local actions = require("ui/elements/common_info_menu_table")
local version = require("version"):getNormalizedCurrentVersion()

assert(not QuickStart:isShown())
local forwarded
ReaderUI.showReader = function(self, path)
    assert(self == ReaderUI)
    forwarded = path
end
actions.quickstart_guide.callback()
assert(forwarded and forwarded == QuickStart.quickstart_filename)
assert(forwarded:sub(1, #DataStorage:getDataDir() + 6) == DataStorage:getDataDir() .. "/help/")
assert(G_reader_settings:readSetting("quickstart_shown_version") == version)
assert(QuickStart:isShown())
local file = assert(io.open(forwarded, "r"))
local html = file:read("*a")
file:close()
assert(html:find("<title>KOReader Quickstart Guide</title>", 1, true))
for _, anchor in ipairs({ "ui", "reading", "dicts" }) do
    assert(html:find('href="#' .. anchor .. '"', 1, true))
end
local first = forwarded
forwarded = nil
actions.quickstart_guide.callback()
assert(forwarded == first)
print("Stock Help callback generates the local guide, forwards it to ReaderUI, and records that it was shown")
