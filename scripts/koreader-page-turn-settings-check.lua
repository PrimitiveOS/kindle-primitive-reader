-- Replay the page-turn controls through the pinned KOReader Mac runtime.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
G_reader_settings = require("luasettings"):wrap({})
local Device = require("device")
require("document/canvascontext"):init(Device)
local Input = require("ui/widget/container/inputcontainer")
local ReaderUI = require("apps/reader/readerui")
local View = require("apps/reader/modules/readerview")
local Rolling = require("apps/reader/modules/readerrolling")
local Paging = require("apps/reader/modules/readerpaging")
local menu = require("ui/elements/page_turns").sub_item_table
local UIManager = require("ui/uimanager")
local gestures = assert(loadfile("plugins/gestures.koplugin/main.lua"))()

local changes = 0
local footer_direction
local view = setmetatable({
    inverse_reading_order = false,
    invert_ui_layout = false,
    footer = { invertProgressBar = function(_, rtl) footer_direction = rtl end },
    setupTouchZones = function() changes = changes + 1 end,
}, { __index = View })
ReaderUI.instance = { view = view }
UIManager.broadcastEvent = function(_, event)
    assert(event.handler == "onToggleUILayoutMirroring" or event.handler == "onToggleUILayoutMiroring")
    assert(view[event.handler])(view)
end

assert(menu[1].checked_func() and menu[2].checked_func() and menu[3].enabled_func())
assert(not menu[3].checked_func() and menu[4].enabled_func())
assert(menu[4].text_func() == "Tap zones: default")
local forward, backward = view:getTapZones()
assert(forward.ratio_x == .25 and forward.ratio_w == .75)
assert(backward.ratio_x == 0 and backward.ratio_w == .25)
menu[1].callback()
assert(not menu[1].checked_func() and not menu[4].enabled_func())
menu[1].callback()

local destination
local reader = { view = view, onGotoViewRel = function(_, relative) destination = relative end }
local gesture = { direction = "west" }
for _, provider in ipairs({Rolling, Paging}) do
    local zone = provider == Rolling and "rolling_swipe" or "paging_swipe"
    local input = {
        _ordered_touch_zones = {
            { def = { id = zone },
              gs_range = { match = function() return true end },
              handler = function(event) return provider.onSwipe(reader, nil, event) end },
        },
        isGestureAlwaysActive = function(_, id)
            return gestures:isGestureAlwaysActive(id)
        end,
    }
    assert(Input.onGesture(input, gesture) and destination == 1)
    destination = nil
    assert(not Input._onGestureFiltered(input, gesture) and destination == nil)
    menu[3].callback()
    assert(menu[3].checked_func() and Input._onGestureFiltered(input, gesture) and destination == 1)
    menu[2].callback()
    destination = nil
    assert(not menu[3].enabled_func() and not menu[3].checked_func())
    assert(not Input._onGestureFiltered(input, gesture) and destination == nil)
    menu[2].callback()
    assert(menu[3].checked_func())
    menu[3].callback()
end

local sub = menu[4].sub_item_table
sub[2].callback()
local dialog
UIManager.show = function(_, instance) dialog = instance end
sub[5].callback()
dialog.callback(80, 53)
forward, backward = view:getTapZones()
assert(math.abs(forward.ratio_x - .47) < 1e-8 and math.abs(backward.ratio_w - .47) < 1e-8)
sub[5].callback()
dialog.callback(20, 50)
forward, backward = view:getTapZones()
assert(forward.ratio_x == .5 and backward.ratio_w == .2)
sub[3].callback()
forward, backward = view:getTapZones()
assert(forward.ratio_y == .5 and backward.ratio_y == 0)
sub[4].callback()
forward, backward = view:getTapZones()
assert(forward.ratio_y == 0 and backward.ratio_y == .8)
sub[1].callback()
assert(not sub[5].enabled_func())

assert(not menu[5].checked_func() and not menu[6].checked_func())
menu[5].callback()
assert(menu[5].checked_func() and not menu[6].checked_func())
forward, backward = view:getTapZones()
assert(forward.ratio_x == 0 and backward.ratio_x == .75)
assert(Rolling.onSwipe(reader, nil, gesture) and destination == -1)
assert(Paging.onSwipe(reader, nil, { direction = "east" }) and destination == 1)
menu[6].callback()
assert(menu[6].checked_func() and footer_direction == true)
menu[5].hold_callback()
dialog.choice2_callback()
menu[6].hold_callback()
dialog.choice2_callback()
assert(G_reader_settings:isTrue("inverse_reading_order"))
assert(G_reader_settings:isTrue("invert_ui_layout"))
menu[5].hold_callback()
dialog.choice1_callback()
menu[6].hold_callback()
dialog.choice1_callback()
assert(not G_reader_settings:isTrue("inverse_reading_order"))
assert(not G_reader_settings:isTrue("invert_ui_layout"))
assert(menu[5].checked_func() and menu[6].checked_func())
assert(changes == 7)
print("Stock tap and swipe switches, locked swipes, tap zone shapes and clamp, inspection and direction defaults verified")
