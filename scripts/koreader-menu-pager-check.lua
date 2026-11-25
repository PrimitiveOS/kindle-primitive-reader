-- Run with the pinned native Mac KOReader runtime and a scratch KO_HOME.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
G_reader_settings = require("luasettings"):wrap({ flash_ui = false })
local Device = require("device")
require("document/canvascontext"):init(Device)
local Event = require("ui/event")
local UIManager = require("ui/uimanager")
local TouchMenu = require("ui/widget/touchmenu")
local Blitbuffer = require("ffi/blitbuffer")
local output = assert(arg[1], "Supply a screenshot directory")
os.time = function() return 12 * 3600 end

local tab = { icon = "appbar.settings" }
for number = 1, 48 do
    tab[number] = { text = ("Settings row %02d"):format(number) }
end
local menu = TouchMenu:new{
    width = Device.screen:getWidth(),
    tab_item_table = { tab },
}
menu:switchMenuTab(1)
UIManager:show(menu)

local function gesture(button, kind)
    UIManager:forceRePaint()
    local box = button.dimen
    button:handleEvent(Event:new("Gesture", {
        ges = kind,
        pos = {
            x = box.x + math.floor(box.w / 2),
            y = box.y + math.floor(box.h / 2),
            w = 0, h = 0,
        },
    }))
end

local function capture(name)
    local framebuffer = Device.screen.bb
    framebuffer:fill(Blitbuffer.COLOR_WHITE)
    UIManager:setDirty(menu, "ui")
    UIManager:forceRePaint()
    framebuffer:writePNG(output .. "/" .. name .. ".png", false)
end

local previous, next_page = menu.page_info_left_chev, menu.page_info_right_chev
local last = menu.page_num
assert(last >= 3 and menu.page == 1 and not previous.enabled)
gesture(previous, "hold")
gesture(previous, "hold_release")
assert(menu.page == 1)

gesture(next_page, "tap")
assert(menu.page == 2)
gesture(next_page, "hold")
assert(menu.page == last and not next_page.enabled)
gesture(next_page, "hold_release")
assert(menu.page == last)
capture("stock-menu-pager-last")

gesture(previous, "tap")
assert(menu.page == last - 1)
gesture(previous, "hold")
assert(menu.page == 1 and not previous.enabled)
gesture(previous, "hold_release")
assert(menu.page == 1)
capture("stock-menu-pager-first")
UIManager:close(menu)
print("Stock menu pager verified: taps step, holds jump to edges, and release adds no turn")
