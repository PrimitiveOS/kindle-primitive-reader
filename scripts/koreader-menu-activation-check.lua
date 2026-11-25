-- Run in the pinned native Mac KOReader runtime with a scratch KO_HOME.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
G_reader_settings = require("luasettings"):wrap({})
local Device = require("device")
require("document/canvascontext"):init(Device)
local ReaderUI = require("apps/reader/readerui")
local ReaderMenu = require("apps/reader/modules/readermenu")
local ReaderConfig = require("apps/reader/modules/readerconfig")
local UIManager = require("ui/uimanager")
local TouchMenu = require("ui/widget/touchmenu")

local events, zones = {}, {}
local function record(value) events[#events + 1] = value end
local function expect(...)
    local expected = {...}
    assert(#events == #expected, table.concat(events, ","))
    for i, item in ipairs(expected) do
        assert(events[i] == item, table.concat(events, ","))
    end
    events = {}
end

local ui = {
    registerTouchZones = function(_, definition)
        for _, zone in ipairs(definition) do zones[zone.id] = zone end
    end,
    handleEvent = function(_, event) record(event.handler) end,
}
local menu = setmetatable({
    ui = ui,
    activation_menu = "swipe_tap",
    tab_item_table = { {}, {}, {} },
    last_tab_index = 2,
    onShowMenu = function(_, tab) record("top " .. tab) end,
}, { __index = ReaderMenu })
local config = setmetatable({
    ui = ui,
    activation_menu = "swipe_tap",
    onShowConfigMenu = function() record("bottom") end,
}, { __index = ReaderConfig })
ui.menu, ui.config = menu, config
ReaderUI.instance = ui
local rows = require("ui/elements/menu_activate").sub_item_table
assert(rows[1].checked_func() and rows[2].checked_func() and rows[3].checked_func())

menu:initGesListener()
config:initGesListener()
local function geometry(id, x, y, w, h)
    local zone = zones[id].screen_zone
    assert(zone.ratio_x == x and zone.ratio_y == y and zone.ratio_w == w and zone.ratio_h == h, id)
end
geometry("readermenu_tap", 0, 0, 1, 1/8)
geometry("readermenu_ext_tap", 1/4, 0, 1/2, 1/5)
geometry("readerconfigmenu_tap", 0, 7/8, 1, 1/8)
geometry("readerconfigmenu_ext_tap", 1/4, 4/5, 1/2, 1/5)
local midpoint = Device.screen:getWidth() / 2
local point = {direction = "south", pos = { x = midpoint, y = 10 }}
assert(zones.readermenu_tap.handler(point))
expect("onShowConfigMenu", "top 2")
assert(zones.readermenu_swipe.handler(point))
expect("onShowConfigMenu", "top 2", "onHandledAsSwipe")

for _, case in ipairs({{1,"swipe"}, {2,"tap"}, {2,"swipe_tap"},
    {2,"tap"}, {1,"swipe"}, {1,"swipe_tap"}}) do
    rows[case[1]].callback()
    assert(menu.activation_menu == case[2] and config.activation_menu == case[2])
    assert(G_reader_settings:readSetting("activate_menu") == case[2])
    assert(rows[1].checked_func() or rows[2].checked_func())
    if case[2] == "swipe" then
        assert(not zones.readermenu_tap.handler(point))
        assert(not zones.readerconfigmenu_tap.handler(point))
        expect()
    elseif case[2] == "tap" then
        assert(not zones.readermenu_swipe.handler(point))
        assert(not zones.readerconfigmenu_swipe.handler({direction="north"}))
        expect()
    end
end

rows[3].callback()
assert(not rows[3].checked_func())
assert(zones.readermenu_tap.handler(point))
expect("top 2")
assert(zones.readermenu_swipe.handler(point))
expect("top 2", "onHandledAsSwipe")
assert(zones.readerconfigmenu_tap.handler(point))
expect("bottom")
assert(zones.readerconfigmenu_swipe.handler({direction="north"}))
expect("bottom")
rows[3].callback()
assert(rows[3].checked_func())

local time = os.time
os.time = function() return 12 * 3600 end
local view = TouchMenu:new{
    width = Device.screen:getWidth(),
    tab_item_table = { { icon = "appbar.settings", unpack(rows) } },
}
view:switchMenuTab(1)
UIManager:show(view)
UIManager:forceRePaint()
Device.screen.bb:writePNG(assert(arg[1]) .. "/stock-menu-activation.png", false)
UIManager:close(view)
os.time = time
print("Stock menu switches, edge geometry, top and direct bottom routing, and auto-show verified")
