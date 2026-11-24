-- Run Lua 5.1 with the pinned perception expander source and widget directory.
local source, widgets = assert(arg[1]), assert(arg[2])
for name, file in pairs{
    ["ui/widget/widget"] = "widget",
    ["ui/widget/container/widgetcontainer"] = "widgetcontainer",
    ["ui/widget/linewidget"] = "linewidget",
    ["ui/widget/horizontalgroup"] = "horizontalgroup",
} do
    package.preload[name] = assert(loadfile(widgets .. "/" .. file .. ".lua"))
end

local width, height = 1264, 1680
local saved, flushes, shown = {}, 0, nil
local function widget() return { new = function(_, item) return item end } end
local function module(name, data) package.preload[name] = function() return data end end
module("ui/widget/eventlistener", { extend = function(_, item) return item end })
module("ui/geometry", widget())
module("ui/widget/multiinputdialog", {new = function(_, item)
    item.onShowKeyboard = function() end
    item.onClose = function() end
    return item
end})
module("ui/widget/infomessage", widget())
module("ui/uimanager", {show = function(_, item) shown = item end, close = function() end})
module("ui/bidi", {mirroredUILayout = function() return false end})
module("util", {})
module("device", {screen = {
    getWidth = function() return width end,
    getHeight = function() return height end,
    getScreenMode = function() return "portrait" end,
}})
module("ffi/util", {template = function(message, value) return (message:gsub("%%1", tostring(value))) end})
module("gettext", function(text) return text end)
module("datastorage", {getSettingsDir = function() return "/tmp" end})
module("luasettings", {open = function() return {
    isTrue = function() return false end,
    saveSetting = function(_, key, value) saved[key] = value end,
    flush = function() flushes = flushes + 1 end,
} end})
module("ffi/blitbuffer", {COLOR_BLACK = 0, gray = function(level) return 255 - math.floor(255 * level) end})

local expander = assert(loadfile(source))():new{}
local menu = {}
expander:addToMainMenu(menu)
local settings = menu.speed_reading_module_perception_expander.sub_item_table
local function frame()
    local lines = {}
    expander:paintTo({paintRect = function(_, x, y, w, h, ink)
        lines[#lines + 1] = table.concat({math.floor(x), y, w, h, ink}, ",")
    end}, 0, 0)
    return table.concat(lines, ";")
end
assert(frame() == "" and not settings[1].checked_func())
settings[1].callback()
assert(settings[1].checked_func())
assert(frame() == "126,84,2,1512,179;1139,84,2,1512,179")
settings[2].callback()
assert(#shown.fields == 4)
shown.buttons[1][1].callback()
assert(saved.shift_each_pages == 100)
shown.getFields = function() return {"", "", "", "2"} end
shown.buttons[1][2].callback()
for page = 1, 2 do expander:onPageUpdate(page) end
assert(frame() == "126,84,2,1512,179;1139,84,2,1512,179")
expander:onPageUpdate(3)
assert(frame() == "164,84,2,1512,179;1101,84,2,1512,179")
for page = 4, 300 do expander:onPageUpdate(page) end
assert(frame():match("^467,84,2,1512,179;"))
settings[2].callback()
shown.getFields = function() return {"4", "0.175", "5", "0"} end
shown.buttons[1][2].callback()
for page = 1, 200 do expander:onPageUpdate(page) end
assert(frame() == "221,84,4,1512,128;1046,84,4,1512,128")
assert(saved.line_thick == 4 and saved.shift_each_pages == 0 and saved.is_enabled)
expander:onFlushSettings()
assert(flushes == 1)
settings[3].callback()
assert(type(shown.text) == "string")
settings[1].callback()
assert(frame() == "")
print("Pinned reading guide draws, settings, enable, shift, stop, persistence and About passed")
