-- Run under the pinned stock runtime with a separate KO_HOME and CRE cache.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local LuaSettings = require("luasettings")
G_reader_settings = LuaSettings:wrap({})
local Device = require("device")
require("document/canvascontext"):init(Device)
local Cre = require("libs/libkoreader-cre")
local CreDocument = require("document/credocument")
local Listener = require("apps/reader/modules/readercoptlistener")
local Rolling = require("apps/reader/modules/readerrolling")
local UIManager = require("ui/uimanager")
local BB = require("ffi/blitbuffer")
local book, cache, font, capture, nonlinear =
    assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4]), assert(arg[5])
local width, height = 1264, 1680

local capacity, charging = 37, false
local power = {
    device = Device,
    getCapacity = function() return capacity end,
    isCharging = function() return charging end,
}
Device.hasBattery = function() return true end
Device.hasAuxBattery = function() return false end
Device.getPowerDevice = function() return power end

local dirty, scheduled, scheduled_after = 0
UIManager.getTopmostVisibleWidget = function() return {name = "ReaderUI"} end
UIManager.setDirty = function(_, _, mode, region)
    assert(mode == "ui" and region.x == 0 and region.y == 0 and region.h > 0)
    dirty = dirty + 1
end
UIManager.scheduleIn = function(_, delay, callback)
    scheduled, scheduled_after = callback, delay
end
UIManager.unschedule = function(_, callback)
    if scheduled == callback then
        scheduled = nil
        return true
    end
end

Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)
local function open(source)
    local view = Cre.newDocView(width, height, 1)
    view:setStyleSheet("./data/epub.css", nil)
    view:setVisiblePageCount(1, false)
    view:setPageMargins(54, 54, 54, 54)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setFontFace("Noto Sans")
    view:setHeaderFont("Noto Sans")
    view:setFontSize(42)
    assert(view:loadDocument(source, false))
    view:renderDocument()
    return view
end
local view = open(book)

local override
local document = setmetatable({
    _document = view,
    configurable = {view_mode = 0, status_line = 0},
    flows = {},
    setPageInfoOverride = function(self, info)
        override = info
        CreDocument.setPageInfoOverride(self, info)
    end,
}, {__index = CreDocument})
local ui = {
    document = document,
    view = {footer = {pageno = 1, settings = {progress_pct_format = 2}}},
    doc_settings = LuaSettings:wrap({}),
    doc_props = {display_title = "Field Notes: Reading Header", authors = "Harbor Reader"},
}
ui.rolling = setmetatable({
    ui = ui,
    view = ui.view,
    onUpdatePos = function() view:renderDocument() end,
}, {__index = Rolling})
ui.handleEvent = function(_, event)
    assert(event.handler == "onSetStatusLine")
    ui.rolling:onSetStatusLine(event.args[1])
end
local listener = Listener:new{document = document, ui = ui, view = ui.view}
listener:onReadSettings(ui.doc_settings)
listener:onReaderReady()
assert(scheduled == listener.headerRefresh and scheduled_after > 1 and scheduled_after <= 61)
assert(view:getHeaderHeight() > 0)

local rows = {}
for _, item in ipairs(listener:getAltStatusBarMenu().sub_item_table) do
    rows[item.text or item.text_func()] = item
end
local battery = assert(rows["Battery status: icon"]).sub_item_table
assert(battery[1].checked_func() and not battery[2].checked_func())
battery[1].callback()
assert(not battery[1].checked_func() and not battery[2].checked_func())
battery[2].callback()
assert(not battery[1].checked_func() and battery[2].checked_func())
rows["Current time"].callback() -- freeze capture contents and test battery-only refresh
assert(not rows["Current time"].checked_func())
local previously_dirty = dirty
scheduled()
assert(dirty == previously_dirty)
charging = true
listener:onCharging()
assert(dirty == previously_dirty + 1)
rows["Progress percentage"].callback()
assert(rows["Progress percentage"].checked_func())

local count = document:getPageCount()
assert(count > 2)
local page = math.floor(count / 2)
ui.view.footer.pageno = page
view:gotoPage(page)
listener:onPageUpdate(page)
local percent = ("%.2f%%"):format(100 * page / count)
assert(override == ("%d / %d  %s  [↯37%%]"):format(page, count, percent), override)
rows["Current page"].callback()
assert(override == ("%d  %s  [↯37%%]"):format(count, percent), override)
rows["Current page"].callback()
rows["Total pages"].callback()
assert(override == ("%d  %s  [↯37%%]"):format(page, percent), override)
rows["Total pages"].callback()

ui.pagemap = {
    wantsPageLabels = function() return true end,
    getCurrentPageLabel = function() return "iii" end,
    getLastPageLabel = function() return "A-19" end,
}
listener:onUsePageLabelsUpdated()
assert(override == ("iii / A-19  %s  [↯37%%]"):format(percent), override)
ui.pagemap = nil

document:setHeaderProgressMarks(count, {math.max(2, math.floor(count / 4)), math.floor(3 * count / 4)})
listener:onPageUpdate(page)
local frame = BB.new(width, height, BB.TYPE_BB8)
view:drawCurrentPage(frame, false, false, false, false)
frame:writePNG(capture, false)
rows["Chapter marks"].callback()
assert(not rows["Chapter marks"].checked_func())
local unmarked = BB.new(width, height, BB.TYPE_BB8)
view:drawCurrentPage(unmarked, false, false, false, false)
local changed = 0
for y = 0, view:getHeaderHeight() + 2 do
    for x = 0, width - 1 do
        if frame:getPixel(x, y):getColor8().a ~= unmarked:getPixel(x, y):getColor8().a then
            changed = changed + 1
        end
    end
end
assert(changed > 0 and changed < width)
unmarked:free()
frame:free()

listener:onSuspend()
assert(scheduled == nil)
listener:onResume()
assert(scheduled == listener.headerRefresh)
ui.view.view_mode = "scroll"
listener:onSetViewMode()
listener:onPageUpdate(page)
assert(scheduled == nil and override == "")
ui.view.view_mode = "page"
listener:onSetViewMode()
assert(scheduled == listener.headerRefresh)
document.configurable.status_line = 1
listener:onSetStatusLine()
listener:onPageUpdate(page)
assert(scheduled == nil and override == "")
view = open(nonlinear)
document._document = view
document.configurable.status_line = 0
listener:onReadSettings(ui.doc_settings)
document:setHideNonlinearFlows(true)
view:renderDocument()
document:cacheFlows()
assert(document:hasHiddenFlows())
local regular, note = document:getFirstPageInFlow(0), document:getFirstPageInFlow(1)
local regular_count, note_count = document:getTotalPagesInFlow(0), document:getTotalPagesInFlow(1)
listener:onPageUpdate(regular)
assert(override == ("1 // %d  %.2f%%  [↯37%%]"):format(regular_count, 100 / regular_count), override)
listener:onPageUpdate(note)
assert(override == ("[1 / %d]1  [%.2f%%]  [↯37%%]"):format(note_count, 100 / note_count), override)
print(("Stock CRE status header passed, page %d of %d (fields, labels, battery, refresh, and flows)"):format(page, count))
