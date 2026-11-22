-- Run from a KOReader installation: ./luajit /path/to/koreader-metadata-autosave.lua
-- Load the real ReaderView, supplying only its UI collaborators and clock.
package.path = "frontend/?.lua;" .. package.path
local now, interval, writes, tasks
local function module(name, implementation)
    package.preload[name] = function() return implementation end
end
local stubs = {
    "ui/bidi", "device", "ui/font", "ui/geometry", "ui/event", "ui/widget/iconwidget",
    "ui/widget/infomessage", "ui/widget/notification", "apps/reader/modules/readerdogear",
    "apps/reader/modules/readerflipping", "apps/reader/modules/readerfooter",
    "ui/widget/textwidget", "dbg", "logger", "ui/data/optionsutil", "ui/size",
    "util", "ffi/util",
}
for _, name in ipairs(stubs) do module(name, {}) end
module("gettext", function(text) return text end)
module("ffi/blitbuffer", {gray = function(value) return value end})
module("ui/widget/overlapgroup", {extend = function(_, widget) return widget end})
module("ui/time", {s = function(value) return value end})
module("ui/uimanager", {
    getElapsedTimeSinceBoot = function() return now end,
    tickAfterNext = function(_, callback) table.insert(tasks, callback) end,
})
G_defaults = {readSetting = function() return 8 end}
G_reader_settings = {
    nilOrFalse = function() return not interval end,
    readSetting = function() return interval end,
}

local ReaderView = require("apps/reader/modules/readerview")
local view = setmetatable({
    ui = {
        doc_settings = {delSetting = function() end},
        saveSettings = function() writes = writes + 1 end,
    },
    state = {},
    highlight = {},
    recalculate = function() end,
}, {__index = ReaderView})

local function run_tasks()
    for _, task in ipairs(tasks) do task() end
    tasks = {}
end

local count = 0
for _, minutes in ipairs({5, 15, 30, 60}) do
    interval, now, writes, tasks = minutes, 0, 0, {}
    view:onReaderReady()
    now = minutes * 60 - 1
    view:onPageUpdate(2)
    assert(#tasks == 0)
    now = now + 1
    view:onPageUpdate(3)
    assert(#tasks == 1 and writes == 0)
    view:onPageUpdate(4)
    assert(#tasks == 1)
    run_tasks()
    assert(writes == 1)
    now = now + minutes * 60
    view:onPosUpdate(80)
    assert(#tasks == 1)
    run_tasks()
    assert(writes == 2)
    now = now + 3601
    view:onResume()
    view:onPosUpdate(81)
    assert(#tasks == 0 and writes == 2)
    count = count + 1
end
interval, now, writes, tasks = false, 0, 0, {}
view:onReaderReady()
now = 7200
view:onPosUpdate(100)
view:onPageUpdate(9)
assert(#tasks == 0 and writes == 0)
print("KOReader metadata-save cases passed: " .. (count + 1))
