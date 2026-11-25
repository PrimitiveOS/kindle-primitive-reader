-- Run inside the pinned Mac KOReader runtime with KO_HOME set to scratch/profile.
local scratch, source = assert(arg[1]), assert(arg[2])
assert(os.getenv("KO_HOME") == scratch .. "/profile")
require("setupkoenv")
G_defaults = require("luadefaults"):open()
G_reader_settings = require("luasettings"):wrap{ document_metadata_folder = "doc" }
local Device = require("device")
require("document/canvascontext"):init(Device)

local DocSettings = require("docsettings")
local UI = require("ui/uimanager")
local util = require("util")
local lfs = require("libs/libkoreader-lfs")
local metadata = require("ui/elements/common_settings_menu_table").document_metadata_location
local about, book_folder, profile, fingerprint, hashes = unpack(metadata.sub_item_table)
local book = scratch .. "/Field Notes.epub"
assert(util.writeToFile(assert(util.readFromFile(source, "rb")), book))

local visible
local show = UI.show
UI.show = function(_, widget) visible = widget end
about.callback()
assert(visible.text:find("sidecar", 1, true))
assert(book_folder.checked_func() and not hashes.enabled_func())
local settings = DocSettings:open(book)
settings:saveSetting("doc_path", book)
settings:saveSetting("last_xpointer", "/body/DocFragment[1]/body/p[2].12")
settings:saveSetting("doc_props", { title = "Field Notes", authors = "Ada Scout" })
settings:saveSetting("annotations", {{ text = "The cove", note = "High tide" }})
local leaf = DocSettings.getSidecarFilename(book)
local sidecars = {}
for _, location in ipairs{"doc", "dir", "hash"} do
    sidecars[location] = DocSettings:getSidecarDir(book, location) .. "/" .. leaf
end

assert(settings:flush(nil, true) .. "/" .. leaf == sidecars.doc)
assert(lfs.attributes(sidecars.doc, "mode") == "file")
profile.callback()
assert(profile.checked_func() and lfs.attributes(sidecars.doc, "mode") == "file")
settings = DocSettings:open(book)
assert(settings:readSetting("annotations")[1].note == "High tide")
assert(settings:flush(nil, true) .. "/" .. leaf == sidecars.dir)
assert(lfs.attributes(sidecars.doc) == nil)

-- Newest usable copy wins on open, even if the preference points to the older copy.
local before = os.time()
assert(lfs.touch(sidecars.dir, before - 30, before - 30))
assert(util.makePath(DocSettings:getSidecarDir(book, "doc")))
local newer = assert(util.readFromFile(sidecars.dir, "rb")):gsub("High tide", "New tide")
assert(util.writeToFile(newer, sidecars.doc))
assert(lfs.touch(sidecars.doc, before - 15, before - 15))
settings = DocSettings:open(book)
assert(settings:readSetting("annotations")[1].note == "New tide")

fingerprint.callback()
assert(fingerprint.checked_func() and hashes.enabled_func())
assert(settings:flush(nil, true) .. "/" .. leaf == sidecars.hash)
assert(lfs.attributes(sidecars.doc) == nil and lfs.attributes(sidecars.dir) == nil)
local listed = DocSettings.findSidecarFilesInHashLocation()
assert(#listed == 1 and listed[1][1] == sidecars.hash)
hashes.callback()
assert(visible.title:find("hash", 1, true) and visible.text:find("Ada Scout", 1, true))
UI.show = show
if arg[3] then
    UI:show(visible)
    UI:forceRePaint()
    Device.screen.bb:writePNG(arg[3] .. "/stock-book-metadata-hashed.png", false)
    UI:close(visible)
end

local renamed = scratch .. "/Renamed Field Notes.epub"
assert(os.rename(book, renamed))
settings = DocSettings:open(renamed)
assert(settings:readSetting("annotations")[1].note == "New tide")
assert(settings:readSetting("last_xpointer"):find("DocFragment", 1, true))

UI.show = function() end
book_folder.callback()
UI.show = show
assert(book_folder.checked_func() and hashes.enabled_func())
settings = DocSettings:open(renamed)
assert(settings:readSetting("annotations")[1].note == "New tide")
assert(settings:flush(nil, true) == DocSettings:getSidecarDir(renamed, "doc"))
assert(lfs.attributes(sidecars.hash) == nil)
assert(#DocSettings.findSidecarFilesInHashLocation() == 0)

local ffi = require("ffi")
ffi.cdef[[int chmod(const char *path, unsigned short mode); unsigned int geteuid(void);]]
if ffi.C.geteuid() ~= 0 then
    local shelf = scratch .. "/Read-only shelf"
    assert(lfs.mkdir(shelf))
    local readonly = shelf .. "/Field Survey.epub"
    assert(util.writeToFile(assert(util.readFromFile(source, "rb")), readonly))
    local writing = DocSettings:open(readonly)
    local fallback = DocSettings:getSidecarDir(readonly, "dir")
    writing:saveSetting("doc_path", readonly)
    writing:saveSetting("last_xpointer", "/body/DocFragment[1]/body/p[2].12")
    assert(ffi.C.chmod(shelf, 320) == 0) -- 0500
    local ok, path = pcall(writing.flush, writing, nil, true)
    assert(ffi.C.chmod(shelf, 448) == 0) -- 0700
    assert(ok and path == fallback)
end
print("Pinned book-folder, profile and hash writes, freshest record, hashed list, rename restore, lazy migration and read-only fallback passed")
