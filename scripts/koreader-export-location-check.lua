-- Execute KOReader's pinned local export menu and path callbacks with authored metadata.
-- Usage: TZ=UTC luajit scripts/koreader-export-location-check.lua <KOReader source>
local source = assert(arg[1], "Pass the pinned KOReader source checkout")
local function mock(name, value)
    package.preload[name] = function() return value end
end

local shown, chooser, shortcut, exported, last_file, twelve_hour
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
local device = {
    hasBattery = function() return true end,
    hasAuxBattery = function() return false end,
    getPowerDevice = function()
        return {
            getCapacity = function() return 81 end,
            isCharged = function() return false end,
            isCharging = function() return true end,
            getBatterySymbol = function(_, charged, charging, level)
                assert(not charged and charging and level == 81)
                return "Charging"
            end,
        }
    end,
}
mock("device", device)
for _, name in ipairs{ "dispatcher", "logger", "readerhighlight", "clip", "ui/network/manager",
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
local book_info = {
    document = {
        file = "/documents/Books/Field Notes.epub",
        hasHiddenFlows = function() return false end,
        getTotalPagesLeft = function(_, page) return 100 - page end,
    },
    ui = {
        doc_props = { display_title = "Field Notes", authors = "Linh Nguyen", series = "Field Library", series_index = "2" },
        view = { footer = {
            pages = 100, pageno = 25, percent_finished = 0.25,
            settings = { pages_left_includes_current_page = true },
            genSeparator = function() return " | " end,
            getChapterProgress = function() return 0.75 end,
        } },
        toc = { getTocTitleByPage = function() return "Shore" end, getChapterPagesLeft = function() return 8 end },
        statistics = { getTimeForPages = function(_, count) return tostring(count) .. " min" end },
    },
}
setfenv(assert(loadstring(expand)), {
    BookInfo = book_info, _ = function(value) return value end,
    G_reader_settings = {
        readSetting = function(_, key) assert(key == "lastfile") return last_file end,
        isTrue = function(_, key) assert(key == "twelve_hour_clock") return twelve_hour end,
    },
    Device = device,
    datetime = { secondsToHour = function(timestamp, twelve)
        return os.date(twelve and "%I:%M %p" or "%H:%M", timestamp)
    end },
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
assert(book_info:expandString("%T/%A/%S/%t/%c/%p/%C/%l/%P/%H/%h", book_info.document.file, when)
    == "Field Notes/Linh Nguyen/Field Library #2/100/25/25/Shore/9/75/75 min/8 min")
assert(book_info:expandString("%f/%F/%b/%B/%r/%D/%d/%m/%M", book_info.document.file, when)
    == "Field Notes.epub//documents/Books/Field Notes.epub/81/Charging/ | /2024-02-29/02-29/10:15/10-15-02")
twelve_hour = true
assert(book_info:expandString("%m", book_info.document.file, when) == "10:15 AM")
twelve_hour = false
book_info.ui.doc_props.display_title = "Field %A"
assert(book_info:expandString("%% %%T %Z %1 %é %T %", book_info.document.file, when)
    == "%% %Field %A N/A %1 %é Field %A %")
book_info.ui.doc_props.display_title = "Field Notes"
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
last_file = book_info.document.file
exporter.settings.clipping_filename_multiple = "Notebook %T"
exporter:exportClippings(multi)
assert(exported[1] == "/documents/Study/Notebook Field Notes" and exported[2] == 2)
exporter.settings.clipping_filename_multiple = "Notebook %D"
alongside.callback()
assert(not alongside.checked_func())

names.callback()
dialog = shown
assert(dialog.fields[1].text == "%T from %f" and dialog.fields[2].text == "Notebook %D")
dialog.values = { "Discard", "Discard" }
dialog.buttons[2][1].callback()
assert(exporter.settings.clipping_filename_single == "%T from %f" and exporter.settings.clipping_filename_multiple == "Notebook %D")
names.callback()
dialog = shown
local restored
dialog._input_widget = {
    hint = dialog.fields[1].hint,
    setText = function(_, value) restored = value end,
    goToEnd = function() end,
}
dialog.buttons[1][2].callback()
assert(restored == "%D-%M %A - %T")
assert(exporter.settings.clipping_filename_single == "%T from %f")
dialog.values = { "", "%D-%M all-books" }
dialog.buttons[2][2].callback()
assert(exporter.settings.clipping_filename_single == nil and exporter.settings.clipping_filename_multiple == nil)
folder.callback()
chooser.callback(chooser.default)
assert(exporter.settings.clipping_dir == "/reader/clipboard")
os.time = time
print("KOReader’s local export paths, token catalog, escaping, separate templates, Cancel and Default verified")
