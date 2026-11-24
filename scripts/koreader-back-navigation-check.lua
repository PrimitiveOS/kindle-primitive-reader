-- Run from the pinned KOReader Mac runtime with a scratch output directory.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local Settings = require("luasettings")
G_reader_settings = Settings:wrap({})
local Device = require("device")
require("document/canvascontext"):init(Device)
local UIManager = require("ui/uimanager")
local Input = require("device/input")
local InputContainer = require("ui/widget/container/inputcontainer")
local Geom = require("ui/geometry")
local ReaderBack = require("apps/reader/modules/readerback")
local ReaderLink = require("apps/reader/modules/readerlink")
local ReaderPaging = require("apps/reader/modules/readerpaging")
local ReaderView = require("apps/reader/modules/readerview")
local menu = require("ui/elements/common_settings_menu_table")
require("apps/filemanager/filemanager")
local FileChooser = require("ui/widget/filechooser")
local output = assert(arg[1])

local function select_choice(name, index, value)
    local item = menu[name].sub_item_table[index]
    item.callback()
    assert(item.checked_func() and G_reader_settings:readSetting(name) == value)
end

assert(menu.back_to_exit.text_func() == "Back to exit: prompt")
assert(menu.back_in_filemanager.text_func() == "Back in file browser: back to exit")
assert(menu.back_in_reader.text_func() == "Back in reader: previous location")
for i, value in ipairs({"prompt", "always", "disable"}) do
    select_choice("back_to_exit", i, value)
    assert(menu.back_in_reader.sub_item_table[1].text_func() == "Back to exit (" .. value .. ")")
    assert(menu.back_in_filemanager.sub_item_table[1].text_func() == "Back to exit (" .. value .. ")")
end
for i, value in ipairs({"default", "parent_folder"}) do
    select_choice("back_in_filemanager", i, value)
end
for i, value in ipairs({"default", "filebrowser", "previous_location", "previous_read_page"}) do
    select_choice("back_in_reader", i, value)
end

-- SDL has a Backspace key. Check the real input setup using a fresh event group.
assert(Device:hasKeyboard() and not Device:hasSymKey())
assert(menu.backspace_as_back and not menu.backspace_as_back.checked_func())
local restart_requests = 0
local ask_for_restart = UIManager.askForRestart
UIManager.askForRestart = function() restart_requests = restart_requests + 1 end
menu.backspace_as_back.callback()
local keyboard = Input:new{ device = Device, group = { Back = {"Back"} } }
assert(menu.backspace_as_back.checked_func() and keyboard.group.Back[2] == "Backspace")
menu.backspace_as_back.callback()
assert(not menu.backspace_as_back.checked_func() and restart_requests == 2)
UIManager.askForRestart = ask_for_restart

local pending
local next_tick = UIManager.nextTick
UIManager.nextTick = function(_, callback) pending = callback end
local function settle()
    if pending then
        local callback = pending
        pending = nil
        callback()
    end
end

local backdrop = InputContainer:new{}
UIManager:show(backdrop)
local function top()
    return UIManager._window_stack[#UIManager._window_stack].widget
end
local function dismiss()
    assert(top() ~= backdrop)
    UIManager:close(top())
end

local location, last_event = "opening", nil
local link_selected = false
local ui = {
    key_events = {},
    document = { info = { has_pages = false } },
    rolling = { getBookLocation = function() return location end },
    handleEvent = function(self, event)
        last_event = event.handler
        if event.handler == "onRestoreBookLocation" then
            if self.paging then
                self.paging:onRestoreBookLocation(event.args[1])
            else
                location = event.args[1].xpointer
            end
        end
    end,
}
ui.link = setmetatable({
    ui = ui,
    location_stack = {},
    forward_location_stack = {},
    isPageLinkSelected = function() return link_selected end,
    clearSelectedPageLink = function() link_selected = false end,
}, { __index = ReaderLink })
local back = ReaderBack:new{ ui = ui }
back:onReadSettings()
assert(ui.key_events.Back[1][1][1] == "Back")
select_choice("back_in_reader", 3, "previous_location")
select_choice("back_to_exit", 1, "prompt")
ui.link.location_stack = {{ xpointer = "chapter" }, { xpointer = location }}
link_selected = true
assert(back:onBack() and not link_selected and #ui.link.location_stack == 2)
assert(back:onBack() and location == "chapter" and #ui.link.location_stack == 0)
assert(back:onBack() and top().text == "Location history is empty.")
dismiss()
last_event = nil
assert(back:onBack() and top().ok_text == "Exit" and last_event == nil)
UIManager:forceRePaint()
Device.screen.bb:writePNG(output .. "/stock-back-confirmation.png", false)
top():onClose()
assert(top() == backdrop and last_event == nil)

select_choice("back_in_reader", 1, "default")
select_choice("back_to_exit", 3, "disable")
assert(back:onBack() and last_event == nil)
select_choice("back_to_exit", 2, "always")
assert(back:onBack() and last_event == "onClose")
select_choice("back_in_reader", 2, "filebrowser")
assert(back:onBack() and last_event == "onHome")

local folder = setmetatable({
    path = "Documents/Local books",
    changeToPath = function(_, path) last_event = path end,
    onClose = function() last_event = "File browser closed" end,
}, { __index = FileChooser })
select_choice("back_in_filemanager", 2, "parent_folder")
assert(folder:onBack() and last_event == "Documents/Local books/..")
select_choice("back_in_filemanager", 1, "default")
select_choice("back_to_exit", 3, "disable")
last_event = nil
assert(folder:onBack() and last_event == nil)
select_choice("back_to_exit", 1, "prompt")
assert(folder:onBack() and top().ok_text == "Exit" and last_event == nil)
top().ok_callback()
assert(last_event == "File browser closed")
dismiss()
select_choice("back_to_exit", 2, "always")
last_event = nil
folder:onBack()
assert(last_event == "File browser closed" and top() == backdrop)

local view = setmetatable({
    state = { page = 1, pos = 0, zoom = 1, rotation = 0, offset = Geom:new{} },
    visible_area = Geom:new{ x = 0, y = 0, w = 100, h = 120 },
    page_area = Geom:new{ w = 100, h = 240 },
    dialog = backdrop,
}, { __index = ReaderView })
ui.document.info.has_pages = true
ui.paging = setmetatable({
    ui = ui,
    view = view,
    current_page = 1,
    _gotoPage = function(self, page) self.current_page = page end,
    setPagePosition = function() end,
}, { __index = ReaderPaging })
select_choice("back_in_reader", 4, "previous_read_page")
select_choice("back_to_exit", 2, "always")
back:onReadSettings()
back:onReaderReady()
settle()
view.state.zoom = 2
view.visible_area.y = 70
back:onViewRecalculate()
settle()
view.state.rotation = 90
view.visible_area.y = 90
back:onPagePositionUpdated()
settle()
assert(#back.location_stack == 2 and view.state.page == 1)
assert(back:onBack())
settle()
assert(view.state.zoom == 2 and view.state.rotation == 0 and view.visible_area.y == 70)
assert(back:onBack())
settle()
assert(view.state.zoom == 1 and view.state.rotation == 0 and view.visible_area.y == 0)
assert(#back.location_stack == 0)
last_event = nil
assert(back:onBack() and top().text == "Location history is empty." and last_event == nil)
dismiss()
assert(back:onBack() and last_event == "onClose")
UIManager.nextTick = next_tick
print("Stock Back choices, keyboard gate, distinct jump, exact same-page fixed restore, folder and exit callbacks verified")
