-- Run from the pinned KOReader Mac runtime with a private KO_HOME.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
G_reader_settings = require("luasettings"):wrap({})
local Device = require("device")
require("document/canvascontext"):init(Device)
local Detector = require("device/gesturedetector")
local UIManager = require("ui/uimanager")
local time = require("ui/time")
local output = assert(arg[1])
local input = {
    main_finger_slot = 0,
    disable_double_tap = true,
    setTimeout = function() end,
    clearTimeout = function() end,
}
local detector = Detector:new{
    screen = Device.screen,
    input = input,
    clock_id = 1,
    active_contacts = {},
    previous_tap = {},
    contact_count = 0,
}
assert(detector.MULTISWIPE_THRESHOLD == 94 and detector.ges_swipe_interval == time.ms(900))

local function report(contact, x, y, ms, id)
    contact.x, contact.y, contact.timev, contact.id = x, y, time.ms(ms), id
    local gestures = detector:feedEvent{contact}
    return gestures[1]
end

local function stroke(positions, release_at, id)
    local contact = {slot = 0}
    id = id or 1
    for _, p in ipairs(positions) do
        local event = report(contact, p[1], p[2], p[3], id)
        assert(not event or event.ges ~= "multiswipe")
    end
    return report(contact, contact.x, contact.y, release_at, -1)
end

local elbow = {{500, 700, 1}, {510, 540, 65}, {500, 425, 120}, {660, 430, 280}}
local corner = stroke(elbow, 320)
assert(corner.ges == "multiswipe" and corner.multiswipe_directions == "north east")
assert(stroke(elbow, 901).ges == "pan_release")
assert(stroke({{500, 700, 1}, {595, 700, 120}, {595, 795, 240}}, 310).multiswipe_directions == "east south")
assert(stroke({{500, 700, 1}, {594, 700, 120}, {594, 794, 240}}, 310).ges == "swipe")
assert(stroke({{500, 700, 1}, {660, 850, 120}, {810, 690, 240}}, 310).multiswipe_directions == "southeast northeast")
assert(stroke({{500, 700, 1}, {380, 700, 120}}, 200, 1).ges == "swipe")
assert(stroke({{380, 700, 201}, {500, 700, 280}}, 360, 2).ges == "swipe")
assert(stroke({{500, 700, 1}, {500, 550, 120}, {500, 700, 240}}, 310).ges == "pan_release")

local Gestures = dofile("plugins/gestures.koplugin/main.lua")
local emitted
local plugin = setmetatable({
    path = "plugins/gestures.koplugin",
    ui = {document = {}, handleEvent = function(_, event)
        assert(event.handler == "onHandledAsSwipe")
    end},
}, {__index = Gestures})
plugin:loadSettings()
local Dispatcher = require("dispatcher")
Dispatcher:init()
local execute = Dispatcher.execute
Dispatcher.execute = function(_, actions, props)
    assert(props.gesture == corner)
    emitted = actions
end
local menu = {}
plugin:addToMainMenu(menu)
local toggle, presets, custom = unpack(menu.gesture_manager.sub_item_table, 1, 3)
assert(toggle.text == "Turn on multiswipes" and not toggle.checked_func())
assert(not presets.enabled_func() and not custom.enabled_func())
assert(not plugin:multiswipeAction(corner.multiswipe_directions, corner) and not emitted)
toggle.callback()
assert(toggle.checked_func() and presets.enabled_func() and custom.enabled_func())
local preset_names = plugin:genMultiswipeMenu(true)
assert(#preset_names == 35 and plugin:multiswipeAction(corner.multiswipe_directions, corner))
assert(emitted.toc)
local configuration = plugin:genMenu("multiswipe_north_east")
local reset, pass, nothing = unpack(configuration, 1, 3)
local refresh = {updateItems = function() end}
assert(reset.checked_func() and pass.text == "Pass through" and nothing.text == "Nothing")
pass.callback(refresh)
emitted = nil
assert(pass.checked_func() and not plugin:multiswipeAction(corner.multiswipe_directions, corner))
assert(emitted == nil)
nothing.callback(refresh)
assert(nothing.checked_func() and plugin:multiswipeAction(corner.multiswipe_directions, corner))
assert(emitted and not next(emitted))
reset.callback(refresh)
assert(reset.checked_func() and plugin:multiswipeAction(corner.multiswipe_directions, corner))
assert(emitted.toc)

local context = {item_table = plugin:genCustomMultiswipeSubmenu(), updateItems = function() end}
local function shown()
    return UIManager._window_stack[#UIManager._window_stack].widget
end
context.item_table[1].callback(context)
local recorder = shown()
assert(recorder.title == "Multiswipe recorder" and recorder._raw_multiswipe == nil)
recorder:onMultiswipe(nil, corner)
recorder.buttons[1][2].callback()
assert(not next(plugin.custom_multiswipes))
UIManager:close(shown())

local custom_event = stroke({{550, 640, 1}, {700, 650, 120}, {710, 800, 240}, {560, 800, 400}, {560, 640, 560}, {715, 640, 710}}, 760)
assert(custom_event.multiswipe_directions == "east south west north east")
recorder:onMultiswipe(nil, custom_event)
UIManager:forceRePaint()
Device.screen.bb:writePNG(output .. "/stock-multiswipe-recorder.png", false)
recorder.buttons[1][2].callback()
local key = "multiswipe_east_south_west_north_east"
assert(plugin.custom_multiswipes[key] and context.item_table[2].hold_callback)
local previous = UIManager._window_stack[#UIManager._window_stack]
menu.gesture_overview.callback()
assert(shown().title == "Gestures in reader")
UIManager:close(shown())
assert(UIManager._window_stack[#UIManager._window_stack] == previous)
context.item_table[2].hold_callback(context)
shown().ok_callback()
assert(not plugin.custom_multiswipes[key])
UIManager:close(shown())
toggle.callback()
assert(not toggle.checked_func() and not presets.enabled_func() and not custom.enabled_func())
Dispatcher.execute = execute
print("Stock one-contact multiswipe replay, timing, presets, settings gate, recording, removal and overview passed")
