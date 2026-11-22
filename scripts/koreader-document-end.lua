-- Exercise the installed ReaderStatus event without opening its framebuffer or input devices.
-- Run from the KOReader directory: ./luajit /path/to/koreader-document-end.lua
package.path = "frontend/?.lua;" .. package.path
local events, summary, setting, auto, sort
local function event(name) table.insert(events, name) end

local modules = {}
modules["gettext"] = function(s) return s end
modules["device"] = {performHapticFeedback = function() event("end") end}
modules["util"] = {}
modules["ui/quickstart"] = {quickstart_filename = "ignored.epub"}
modules["ui/widget/booklist"] = {setBookInfoCacheProperty = function() event("mark:" .. summary.status) end}
modules["ui/widget/bookstatuswidget"] = {new = function(_, args)
    return {kind = "status", close_callback = args.close_callback}
end}
modules["ui/widget/infomessage"] = {new = function(_, args) return {kind = "notice", text = args.text} end}
modules["ui/widget/buttondialog"] = {new = function(_, args)
    args.kind = "dialog"
    return args
end}
modules["ui/widget/container/widgetcontainer"] = {extend = function(_, target) return target end}
modules["apps/filemanager/filemanager"] = {showDeleteFileDialog = function() event("confirm-delete") end}
local shown
modules["ui/uimanager"] = {
    getTopmostVisibleWidget = function() return shown end,
    nextTick = function(_, callback) callback() end,
    show = function(_, widget) shown = widget; event(widget.kind) end,
    close = function() shown = nil end,
    forceRePaint = function() end,
}
for name, implementation in pairs(modules) do
    package.preload[name] = function() return implementation end
end
G_reader_settings = {
    readSetting = function(_, key)
        return ({lastfile = "document.epub", end_document_action = setting, collate = sort})[key]
    end,
    isTrue = function(_, key) return key == "end_document_auto_mark" and auto end,
}

local reader = require("apps/reader/modules/readerstatus")
reader.document = {file = "document.epub"}
reader.ui = {
    doc_settings = {
        readSetting = function() return summary end,
        flush = function() event("flush") end,
    },
    gotopage = {onGoToBeginning = function() event("beginning") end},
}
reader.openFileBrowser = function() event("library") end
reader.onOpenNextOrPreviousFileInFolder = function() event("next") end

local count = 0
local function check(action, expected, options, interact)
    options = options or {}
    events, summary, shown = {}, {status = options.status or "reading"}, nil
    setting, auto, sort = action, options.auto or false, options.sort or "natural"
    reader:onEndOfBook()
    if interact then interact() end
    local result = table.concat(events, ",")
    assert(result == "end," .. expected or (expected == "" and result == "end"), result)
    count = count + 1
end
check(nil, "dialog", nil, function()
    assert(#shown.buttons == 3)
    for _, row in ipairs(shown.buttons) do assert(#row == 2) end
end)
check("nothing", "")
check("nothing", "mark:complete", {auto = true})
check("book_status", "status")
check("next_file", "notice,next")
check("next_file", "notice", {sort = "date"})
check("goto_beginning", "beginning")
check("file_browser", "library")
check("mark_read", "mark:complete,notice")
check("book_status_file_browser", "status,library", nil, function() shown.close_callback() end)
check("delete_file", "flush,confirm-delete")
check("pop-up", "dialog,mark:reading", {status = "complete"}, function() shown.buttons[1][1].callback() end)
check("pop-up", "dialog,flush,confirm-delete", nil, function() shown.buttons[3][1].callback() end)
print("KOReader document end cases passed: " .. count)
