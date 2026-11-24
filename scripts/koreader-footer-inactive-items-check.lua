-- Execute inside the pinned KOReader Mac runtime with TZ=UTC and a scratch KO_HOME.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
G_reader_settings = require("luasettings"):wrap({})
local Device = require("device")
require("document/canvascontext"):init(Device)
local ReaderFooter = require("apps/reader/modules/readerfooter")

local generators
for index = 1, 20 do
    local name, value = debug.getupvalue(ReaderFooter.updateFooterTextGenerator, index)
    if name == "footerTextGeneratorMap" then
        generators = value
        break
    end
end
assert(generators)

local brightness, warmth, annotations, percentage = 0, 50, 0, 81
local power = {
    isFrontlightOn = function() return brightness > 0 end,
    frontlightIntensity = function() return brightness end,
    frontlightWarmth = function() return warmth end,
    getCapacity = function() return percentage end,
    isCharging = function() return true end,
}
Device.getPowerDevice = function() return power end
Device.hasBattery = function() return true end
Device.hasAuxBattery = function() return false end
local settings = {
    all_at_once = true,
    hide_empty_generators = false,
    battery_hide_threshold = 80,
    item_prefix = "letters",
    items_separator = "bar",
}
local footer = setmetatable({
    settings = settings,
    footerTextGenerators = {
        generators.bookmark_count,
        generators.page_turning_inverted,
        generators.frontlight,
        generators.frontlight_warmth,
    },
    ui = {
        annotation = { getNumberOfAnnotations = function() return annotations end },
    },
    view = {},
}, { __index = ReaderFooter })

local function normalize(value)
    return (value:gsub("\226\129[\166-\169]", ""))
end

assert(normalize(footer:genAllFooterText()) == "BM: 0 | Pg: Off | L: Off | LW: Off")
settings.hide_empty_generators = true
assert(footer:genAllFooterText() == "")
assert(generators.battery(footer) == "")
percentage = 80
assert(normalize(generators.battery(footer)) == "B: +80%")
percentage, brightness, annotations = 81, 7, 2
assert(normalize(footer:genAllFooterText()) == "BM: 2 | L: 7 | LW: 50%")
settings.all_at_once, brightness = false, 0
assert(generators.frontlight(footer) == "L: Off")
assert(generators.frontlight_warmth(footer) == "LW: Off")
assert(generators.page_turning_inverted(footer) == "Pg: Off")
assert(normalize(generators.battery(footer)) == "B: +81%")

local time = os.time
local cases = {
    { 9 * 60, "0:09", "12:09 AM" },
    { 12 * 3600 + 9 * 60, "12:09", "12:09 PM" },
    { 23 * 3600 + 9 * 60, "23:09", "11:09 PM" },
}
for _, case in ipairs(cases) do
    os.time = function() return case[1] end
    G_reader_settings:saveSetting("twelve_hour_clock", false)
    assert(generators.time(footer) == case[2])
    G_reader_settings:saveSetting("twelve_hour_clock", true)
    assert(generators.time(footer) == case[3])
end
os.time = time
print("Stock inactive and reactivated items, battery boundary and single-item scope, midnight/noon clock verified")
