-- Execute KOReader's pinned local export menu and path callbacks with authored metadata.
-- Usage: TZ=UTC luajit scripts/koreader-export-location-check.lua <KOReader source>
local source = assert(arg[1], "Pass the pinned KOReader source checkout")
local function mock(name, value)
    package.preload[name] = function() return value end
end

local shown, chooser, shortcut, exported
local widget = { new = function(_, options) return options end }
local text = {
    name = "text",
    getMenuTable = function(self) return { text = self.name } end,
    isEnabled = function() return true end,
    export = function(self, books)
        exported = { self.filepath, #books }
        return true
    end,
    getFilePath = function(self) return self.filepath .. ".txt" end,
}
mock("gettext", function(value) return value end)
mock("datastorage", { getFullDataDir = function() return "/reader" end })
mock("ui/widget/infomessage", widget)
mock("ui/widget/multiinputdialog", {
    new = function(_, options)
        options.onShowKeyboard = function() end
        options.getFields = function(self) return self.values end
        return options
    end,
})
mock("ui/uimanager", {
    show = function(_, value) shown = value end,
    close = function() shown = nil end,
    nextTick = function(_, run) run() end,
})
mock("ui/widget/container/widgetcontainer", {
    extend = function(_, value) return value end,
})
mock("apps/filemanager/filemanagerutil", {
    showChooseDialog = function(_, callback, current, default)
        chooser = { callback = callback, current = current, default = default }
    end,
})
mock("ffi/util", {
    dirname = function(path) return path:match("^(.*)/[^/]+$") end,
    template = function(pattern, ...)
        local values = { ... }
        return (pattern:gsub("%%(%d)", function(i) return tostring(values[tonumber(i)]) end))
    end,
})
mock("util", { getSafeFilename = function(value) return value end })
mock("provider", { size = function() return 0 end })
for _, name in ipairs{ "device", "dispatcher", "logger", "readerhighlight", "clip", "ui/network/manager",
    "ui/widget/buttonselector", "apps/reader/modules/readerhighlight" } do
    mock(name, {})
end
for _, name in ipairs{ "html", "joplin", "json", "markdown", "my_clippings", "nextcloud", "readwise", "text", "xmnote" } do
    mock("target/" .. name, name == "text" and text or {
        name = name,
        getMenuTable = function(self) return { text = self.name } end,
        isEnabled = function() return false end,
    })
end

-- Load the actual expansion method with an in-memory fixture for its active document.
local f = assert(io.open(source .. "/frontend/apps/filemanager/filemanagerbookinfo.lua"))
local book_info_source = f:read("*all")
f:close()
local expand = assert(book_info_source:match("(function BookInfo:expandString%b()%s+.-\nend)"))
local book_info = { document = { file = "/documents/Books/Field Notes.epub" },
    ui = { doc_props = { display_title = "Field Notes", authors = "Linh Nguyen" },
           view = { footer = { percent_finished = 0.25 } } } }
setfenv(assert(loadstring(expand)), {
    BookInfo = book_info, _ = function(value) return value end,
    G_reader_settings = { readSetting = function() return nil end },
    Math = { round = function(v) return math.floor(v + 0.5) end },
    os = os,
})()

local exporter = dofile(source .. "/plugins/exporter.koplugin/main.lua")
exporter.settings = {}
exporter.ui = { bookinfo = book_info, folder_shortcuts = {
    updateShortcut = function(_, name, value) shortcut = { name, value } end,
} }
local menu = {}
exporter:addToMainMenu(menu)
local function action(label)
    for _, row in ipairs(menu.exporter.sub_item_table) do
        if row.text == label then return row end
    end
    error("Missing stock option " .. label)
end

local when = os.time{ year = 2024, month = 2, day = 29, hour = 10, min = 15, sec = 2 }
local time = os.time
os.time = function() return when end
local single = { book = { file = book_info.document.file } }
local multi = { first = single.book, second = { file = "/documents/Novel.epub" } }
exporter:exportClippings(single)
assert(exported[1] == "/reader/clipboard/2024-02-29-10-15-02 Linh Nguyen - Field Notes" and exported[2] == 1)
exporter:exportClippings(multi)
assert(exported[1] == "/reader/clipboard/2024-02-29-10-15-02 all-books" and exported[2] == 2)

local folder = action("Choose export folder")
folder.callback()
assert(chooser.current == nil and chooser.default == "/reader/clipboard")
chooser.callback("/documents/Study")
assert(exporter.settings.clipping_dir == "/documents/Study")
assert(shortcut[1] == "exporter" and shortcut[2] == "/documents/Study")
folder.callback()
assert(chooser.current == "/documents/Study")

local names = action("Set export filename")
names.callback()
assert(shown.fields[1].text == "%D-%M %A - %T")
assert(shown.fields[2].text == "%D-%M all-books")
local dialog = shown
dialog.values = { "%T from %f", "Notebook %D" }
dialog.buttons[2][2].callback()
exporter:exportClippings(single)
assert(exported[1] == "/documents/Study/Field Notes from Field Notes.epub")
local alongside = action("Use book folder for single export")
alongside.callback()
assert(alongside.checked_func())
exporter:exportClippings(single)
assert(exported[1] == "/documents/Books/Field Notes from Field Notes.epub")
exporter:exportClippings(multi)
assert(exported[1] == "/documents/Study/Notebook 2024-02-29" and exported[2] == 2)
alongside.callback()
assert(not alongside.checked_func())

names.callback()
dialog = shown
assert(dialog.fields[1].text == "%T from %f" and dialog.fields[2].text == "Notebook %D")
dialog.values = { "", "%D-%M all-books" }
dialog.buttons[2][2].callback()
assert(exporter.settings.clipping_filename_single == nil and exporter.settings.clipping_filename_multiple == nil)
folder.callback()
chooser.callback(chooser.default)
assert(exporter.settings.clipping_dir == "/reader/clipboard")
os.time = time
print("KOReader local export folder, independent naming templates, reset and single-book folder precedence verified")
