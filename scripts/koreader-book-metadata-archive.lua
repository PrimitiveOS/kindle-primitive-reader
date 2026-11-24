-- Run with ./luajit from the pinned stock runtime, using an empty KO_HOME.
local scratch, fixture = assert(arg[1]), assert(arg[2])
assert(os.getenv("KO_HOME") == scratch .. "/profile", "Use the temporary profile")
require("setupkoenv")

local Settings = require("luasettings")
local DocSettings = require("docsettings")
local util = require("util")
local lfs = require("libs/libkoreader-lfs")
local bytes = assert(util.readFromFile(fixture, "rb"))
local archive = scratch .. "/archive"
assert(lfs.mkdir(archive))
G_reader_settings = Settings:wrap{ document_metadata_arc_folder = archive }

local source = scratch .. "/Field Notes.epub"
local moved = scratch .. "/Copy of Field Notes.epub"
assert(util.writeToFile(bytes, source))
assert(util.writeToFile(bytes, moved))
local checksum = assert(util.partialMD5(source))
assert(checksum == util.partialMD5(moved))

local settings = DocSettings:open(source)
settings:saveSetting("doc_props", { title = "Field Notes", authors = "Ada Scout" })
settings:saveSetting("partial_md5_checksum", checksum)
settings:saveSetting("last_xpointer", "/body/DocFragment[1]/body/p[2].12")
settings:saveSetting("annotations", {{ text = "The cove", note = "High tide" }})
settings:flush()
local file = assert(DocSettings.getSettingsArcFile(checksum))
DocSettings.saveSettingsArcFile(settings, nil, true)
assert(not DocSettings.getSettingsArcFile(checksum, true))
G_reader_settings:saveSetting("document_metadata_arc_on_closing", true)
DocSettings.saveSettingsArcFile(settings, nil, true)
assert(DocSettings.getSettingsArcFile(checksum, true) == file)
assert(settings:hasNot("metadata_arc"))

-- Stub window and provider collaborators. Run stock archive list and restore callbacks.
local prompt, opened, has_live = nil, 0, false
local stubs = {
    ["device"] = { screen = {}, input = {} },
    ["ui/widget/booklist"] = { hasBookBeenOpened = function() return has_live end },
    ["ui/widget/confirmbox"] = { new = function(_, details) return details end },
    ["ui/widget/container/inputcontainer"] = { extend = function(_, fields) return fields end },
    ["ui/widget/container/widgetcontainer"] = { extend = function(_, fields) return fields end },
    ["ui/event"] = { new = function(_, name) return name end },
    ["ui/uimanager"] = {
        show = function(_, widget) prompt = widget end,
        broadcastEvent = function(_, event) assert(event == "ShowingReader") end,
    },
    ["apps/filemanager/filemanagerutil"] = {
        splitFileNameType = function(path) return path:match("([^/]+)%.") end,
    },
}
local native = {
    ["docsettings"] = true, ["ffi/util"] = true, ["libs/libkoreader-lfs"] = true,
    ["logger"] = true, ["util"] = true, ["gettext"] = true, ["luasettings"] = true,
}
for _, path in ipairs({"frontend/apps/reader/readerui.lua", "frontend/ui/widget/bookmetadataarchive.lua"}) do
    local code = assert(util.readFromFile(path))
    for name in code:gmatch('local%s+[%w_]+%s*=%s*require%("([^"]+)"%)') do
        if not native[name] then package.loaded[name] = stubs[name] or {} end
    end
end

local widget = require("ui/widget/bookmetadataarchive")
local list = widget.getBookList()
assert(#list == 1 and list[1].text == "Ada Scout • Field Notes")
local record = list[1].doc_settings
assert(record:readSetting("metadata_arc").on_closing)
assert(record:readSetting("annotations")[1].note == "High tide")

local reader = require("apps/reader/readerui")
reader.extendProvider = function(_, _, provider) return provider end
reader.showReaderCoroutine = function(_, path)
    assert(path == moved)
    opened = opened + 1
end
local provider = { provider = {} }
has_live = true
reader:showReader(moved, provider)
assert(prompt == nil and opened == 1 and DocSettings.getSettingsArcFile(checksum, true))
has_live = false
reader:showReader(moved, provider)
assert(prompt and prompt.ok_text == "Restore" and prompt.cancel_text == "Discard" and opened == 1)
prompt.ok_callback()
assert(opened == 2 and not DocSettings.getSettingsArcFile(checksum, true))
assert(reader.arc_settings_data.last_xpointer == settings:readSetting("last_xpointer"))
assert(reader.arc_settings_data.annotations[1].note == "High tide")
reader.arc_settings_data = nil

G_reader_settings:saveSetting("document_metadata_arc_on_deletion", true)
DocSettings.updateLocation(source)
assert(assert(util.readFromFile(source, "rb")) == bytes)
assert(not DocSettings:hasSidecarFile(source))
list = widget.getBookList()
assert(#list == 1 and not list[1].doc_settings:readSetting("metadata_arc").on_closing)
reader:showReader(moved, provider)
prompt.cancel_callback()
assert(opened == 3 and reader.arc_settings_data == nil)
assert(not DocSettings.getSettingsArcFile(checksum, true))
print("Pinned KOReader: close and deletion snapshots, list contents, renamed book lookup, live-data priority, restore and discard")
