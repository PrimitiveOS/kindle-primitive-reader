-- Run in the pinned native KOReader runtime. Pass a scratch output directory.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local Settings = require("luasettings")
G_reader_settings = Settings:wrap({})
local Device = require("device")
require("document/canvascontext"):init(Device)
local Screen = Device.screen
local UIManager = require("ui/uimanager")
local Event = require("ui/event")
local InputContainer = require("ui/widget/container/inputcontainer")
local Notification = require("ui/widget/notification")
local menu = require("ui/elements/screen_notification_menu_table")
local ReaderFont = require("apps/reader/modules/readerfont")
local output = assert(arg[1])

local source_bits = {
    Notification.SOURCE_BOTTOM_MENU_ICON, Notification.SOURCE_BOTTOM_MENU_TOGGLE,
    Notification.SOURCE_BOTTOM_MENU_FINE, Notification.SOURCE_BOTTOM_MENU_MORE,
    Notification.SOURCE_BOTTOM_MENU_PROGRESS, Notification.SOURCE_DISPATCHER,
    Notification.SOURCE_OTHER,
}
local defaults = {false, false, true, true, true, true, false}
local function setting()
    return G_reader_settings:readSetting("notification_sources_to_show_mask")
end
assert(Notification.SOURCE_DEFAULT == 0x3c and menu.checked_func())
for index, source in ipairs(source_bits) do
    local item = menu.sub_item_table[index]
    assert(item.checked_func() == defaults[index])
    assert(item.hold_callback == nil)
    item.callback()
    assert(item.checked_func() ~= defaults[index])
    assert(setting() == bit.bxor(Notification.SOURCE_DEFAULT, source))
    item.callback()
    assert(setting() == Notification.SOURCE_DEFAULT)
end

local background = InputContainer:new {
    dismissed = 0,
    onGesture = function(self)
        self.dismissed = self.dismissed + 1
        return true
    end,
}
UIManager:show(background)

local function top()
    return UIManager._window_stack[#UIManager._window_stack].widget
end

local history = menu.sub_item_table[8]
assert(history.keep_menu_open)
history.callback()
assert(top().title == "Past notifications")
assert(top().text == "No notifications available.")
UIManager:close(top())

local function end_gesture()
    local previous = background.dismissed
    UIManager:sendEvent(Event:new("Gesture", { ges = "tap", pos = { x = 30, y = 40 } }))
    assert(top() == background and background.dismissed == previous + 1)
end

local updates = 0
local font = setmetatable({
    configurable = {},
    ui = {
        document = {
            setInterlineSpacePercent = function(_, percent) assert(percent >= 50 and percent <= 200) end,
        },
        handleEvent = function(_, event)
            assert(event.handler == "onUpdatePos")
            updates = updates + 1
        end,
    },
}, { __index = ReaderFont })
for index, source in ipairs(source_bits) do
    Notification:setNotifySource(source)
    assert(font:onSetLineSpace(100 + index))
    Notification:resetNotifySource()
    if defaults[index] then
        assert(top().toast and top().timeout == 2)
        assert(top().text == string.format("Line spacing set to: %d%%.", 100 + index))
        end_gesture()
    else
        assert(top() == background)
    end
end
assert(updates == 7 and #Notification:getPastMessages() == 4)

G_reader_settings:saveSetting("notification_sources_to_show_mask", 0)
assert(not menu.checked_func())
for _, source in ipairs(source_bits) do
    assert(not Notification:notify("Suppressed", source))
end
assert(#Notification:getPastMessages() == 4)
assert(Notification:notify("Important message", Notification.SOURCE_ALWAYS_SHOW))
end_gesture()

-- Freeze just the display clock for a reproducible frame.
local date = os.date
os.date = function(format, stamp)
    if format == "%X: " then return "09:41:00: " end
    return date(format, stamp)
end
for i = 1, 25 do
    assert(Notification:notify(string.format("Test notification %02d", i), Notification.SOURCE_ALWAYS_SHOW))
    end_gesture()
end
os.date = date
local collected = Notification:getPastMessages()
assert(#collected == 20)
assert(collected[1] == "09:41:00: Test notification 25")
assert(collected[20] == "09:41:00: Test notification 06")

history.callback()
assert(top().text == table.concat(collected, "\n"))
UIManager:forceRePaint()
Screen.bb:writePNG(output .. "/stock-past-notifications.png", false)
UIManager:close(top())
print("KOReader verified seven independent masks, four defaults, non-consuming dismissal, important messages, and newest-first 20-message history")
Device:exit()
