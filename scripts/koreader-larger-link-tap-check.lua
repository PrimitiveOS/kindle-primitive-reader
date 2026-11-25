-- Exercise ReaderLink's pinned geometry and policy in the native Mac runtime.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
G_reader_settings = require("luasettings"):wrap({})
local Device = require("device")
require("document/canvascontext"):init(Device)
local ReaderLink = require("apps/reader/modules/readerlink")
local radius = Device.screen:scaleByDPI(30)
assert(radius == 57)

local address = "https://example.org/harbor"
local web = {uri = address, start_x = 100, end_x = 220, start_y = 200, end_y = 200}
local local_note = {section = "#note", segments = {{x0 = 350, x1 = 410, y0 = 200, y1 = 232}}}
local linked = setmetatable({
    ui = {rolling = {}},
    document = {
        getPageLinks = function(_, internal)
            return internal and { local_note } or { web, local_note }
        end,
    },
    getLinkFromGes = function(_, gesture)
        if gesture.pos.x == 120 then return {xpointer = address} end
    end,
    showLinkBox = function(_, target) return target.xpointer end,
    onGotoLink = function(_, target) return target.xpointer end,
}, { __index = ReaderLink })
local function tap(x, y)
    return linked:onTap(nil, {pos={x=x,y=y or 200}})
end
assert(tap(120) == address)
assert(tap(100 - radius) == nil)
G_reader_settings:saveSetting("larger_tap_area_to_follow_links", true)
assert(tap(100 - radius) == address)
assert(tap(100 - radius - 1) == nil)
assert(tap(350, 232 + radius) == "#note")
assert(tap(350, 232 + radius + 1) == nil)
G_reader_settings:saveSetting("tap_ignore_external_links", true)
assert(tap(120) == nil)
assert(tap(350 - radius) == "#note")
G_reader_settings:saveSetting("larger_tap_area_to_follow_links", false)
assert(tap(350 - 1) == nil)
assert(tap(350, 232) == "#note")
print("Stock enlarged tap radius, external policy, and exact internal segments verified")
