-- Run with LuaJIT and the pinned KOReader source tree. No writes or live device services.
local root = assert(arg[1], "Pass the pinned KOReader source tree")
package.path = root .. "/frontend/?.lua;" .. package.path
local function mock(name, value)
    package.preload[name] = function() return value end
end
local function translation(_, value) return value end
mock("gettext", setmetatable({pgettext = translation, npgettext = translation}, {
    __call = function(_, value) return value end,
}))
local function template(text, ...)
    local values = {...}
    return (text:gsub("%%(%d)", function(index) return tostring(values[tonumber(index)]) end))
end
local function ordered(values)
    local keys = {}
    for key in pairs(values) do keys[#keys + 1] = key end
    table.sort(keys)
    local i = 0
    return function()
        i = i + 1
        local key = keys[i]
        if key then return key, values[key] end
    end
end
local function clone(value)
    if type(value) ~= "table" then return value end
    local copy = {}
    for key, entry in pairs(value) do copy[key] = clone(entry) end
    return copy
end
mock("ffi/util", {template = template, orderedPairs = ordered})
mock("util", {
    arrayContains = function(array, searched)
        for index, value in ipairs(array) do if value == searched then return index end end
    end,
    arrayReverse = function(array)
        local out = {}
        for i = #array, 1, -1 do out[#out + 1] = array[i] end
        return out
    end,
    tableGetValue = function(table, ...)
        for _, key in ipairs({...}) do if not table then return end; table = table[key] end
        return table
    end,
    tableSize = function(values)
        local n = 0
        for _ in pairs(values) do n = n + 1 end
        return n
    end,
    tableDeepCopy = clone,
})
local screen = setmetatable({DEVICE_ROTATED_UPRIGHT = 0, DEVICE_ROTATED_CLOCKWISE = 1,
    DEVICE_ROTATED_UPSIDE_DOWN = 2}, {__index = function() return function() return 0 end end})
mock("device", setmetatable({screen = screen, isTouchDevice = function() return true end,
    getPowerDevice = function() return {fl_max = 24, fl_warmth_max = 24} end},
    {__index = function() return function() return false end end}))
mock("datastorage", {getSettingsDir = function() return "unused" end})
mock("ui/data/ocr", {getOCRLangs = function() return {} end})
mock("ui/data/isolanguage", {})
mock("ui/bidi", {mirroredUILayout = function() return false end})
mock("ui/data/optionsutil", setmetatable({rotation_modes = {0, 1, 2, 3}, rotation_labels = {"0", "1", "2", "3"}},
    {__index = function() return function() return true end end}))
mock("ui/event", {new = function(_, name, ...) return {name = name, args = {...}} end})
local notifications = {setNotifySource = function() end, resetNotifySource = function() end}
mock("ui/widget/notification", notifications)
local profile_ui = {instance = {rolling = true}}
mock("apps/reader/readerui", profile_ui)
local events = {}
local visible
mock("ui/uimanager", {
    sendEvent = function(_, event) events[#events + 1] = event end,
    broadcastEvent = function(_, event) events[#events + 1] = event end,
    setSilentMode = function() end,
    show = function(_, widget) visible = widget end,
    close = function() visible = nil end,
})
for _, name in ipairs{"confirmbox", "inputdialog", "infomessage", "datetimewidget", "spinwidget"} do
    mock("ui/widget/" .. name, {new = function(_, widget)
        widget.getInputText = function(self) return self.input end
        widget.onShowKeyboard = function() end
        return widget
    end})
end
mock("ui/widget/container/widgetcontainer", {extend = function(_, options) return options end})
for _, name in ipairs{"apps/filemanager/filemanager", "apps/reader/modules/readerdictionary",
    "apps/reader/modules/readerfooter", "apps/reader/modules/readerhighlight",
    "apps/reader/modules/readertypography", "apps/reader/modules/readerview",
    "apps/reader/modules/readerzooming", "device/key", "luasettings", "ui/size", "fontlist", "logger"} do
    mock(name, {})
end
local defaults = dofile(root .. "/defaults.lua")
G_defaults = {readSetting = function(_, name) return defaults[name] end}
local dispatcher = require("dispatcher")
dispatcher:init()
local stock = dofile(root .. "/plugins/profiles.koplugin/main.lua")

local function value(config, field)
    for _, group in ipairs(require(config)) do
        for _, option in ipairs(group.options) do
            if option.name == field then return option.default_value or option.values[1] end
        end
    end
end
local function document(config)
    local defaults = {}
    for _, group in ipairs(require(config)) do
        for _, option in ipairs(group.options) do
            defaults[option.name] = option.default_value or (option.values and option.values[1])
        end
    end
    return {configurable = defaults}
end
local profiles = setmetatable({
    data = {},
    ui = {rolling = true, font = {font_face = "Harbor Serif"}},
    autoexec = {},
    document = document("ui/data/creoptions"),
}, {__index = stock})
local menu = {updateItems = function() end, onClose = function() end, item_table_stack = {1}}
local function choose(items, label)
    for _, item in ipairs(items) do if item.text == label then return item end end
    error("Missing " .. label)
end
local function stock_name(initial, new_name)
    assert(visible.input == initial)
    visible.input = new_name
    choose(visible.buttons[1], "Save").callback()
end
local root_menu = profiles:getSubMenuItems()
assert(root_menu[1].text == "New" and root_menu[2].enabled)
root_menu[1].callback(menu)
stock_name(nil, "Empty")
assert(profiles.data.Empty.settings.name == "Empty" and next(profiles.data.Empty) == "settings")
root_menu[2].callback(menu)
stock_name(nil, "Text")
local text = profiles.data.Text
assert(text.settings.order[1] == "rotation_mode" and text.set_font == "Harbor Serif")
assert(text.font_size == value("ui/data/creoptions", "font_size") and text.kopt_text_wrap == nil)
assert(text.view_mode ~= nil and text.nightmode_images ~= nil)

profiles.ui.rolling = nil
profiles.document = document("ui/data/koptoptions")
profiles.document.configurable.zoom_overlap_h = 38
root_menu[2].callback(menu)
stock_name(nil, "PDF")
local pdf = profiles.data.PDF
assert(pdf.kopt_zoom_overlap_h == 38 and pdf.set_font == nil and pdf.font_size == nil)
assert(pdf.kopt_page_scroll ~= nil and pdf.kopt_zoom_mode_genus ~= nil)
local commands
for _, item in ipairs(profiles:getSubMenuItems()) do if item.name == "PDF" then commands = item.sub_item_table end end
assert(commands)
local edit
for _, item in ipairs(commands) do
    if item.text_func and item.text_func():match("Edit actions") then edit = item end
end
assert(edit)
local fixed = choose(edit.sub_item_table_func(), "Fixed layout documents (pdf, djvu, pics…)")
local overlap
for _, item in ipairs(fixed.sub_item_table) do
    if item.text_func then
        local ok, label = pcall(item.text_func)
        if ok and label and label:match("^Horizontal overlap") then overlap = item; break end
    end
end
assert(overlap and overlap.checked_func())
overlap.callback(menu)
assert(visible.value == 38 and visible.default_value == value("ui/data/koptoptions", "zoom_overlap_h"))
visible.callback({value = 45})
assert(pdf.kopt_zoom_overlap_h == 45)
choose(commands, "Duplicate").callback(menu)
stock_name("PDF", "PDF copy")
assert(profiles.data["PDF copy"].kopt_zoom_overlap_h == 45)
choose(commands, "Rename: PDF").callback(menu)
stock_name("PDF", "Scanned pages")
assert(not profiles.data.PDF and profiles.data["Scanned pages"].settings.name == "Scanned pages")

local function sent_setting(name)
    for _, event in ipairs(events) do
        if event.name == "ConfigChange" and event.args[1] == name then return event.args[2] end
    end
end
profile_ui.instance = {paging = true}
events = {}
profiles:onProfileExecute("Scanned pages", {qm_show = false})
assert(events[1].name == "BatchedUpdate" and events[#events].name == "BatchedUpdateDone")
assert(sent_setting("zoom_overlap_h") == 45 and sent_setting("embedded_css") == nil)
events = {}
profiles:onProfileExecute("Text", {qm_show = false})
assert(sent_setting("embedded_css") == nil)
profile_ui.instance = {rolling = true}
events = {}
profiles:onProfileExecute("Text", {qm_show = false})
assert(sent_setting("embedded_css") ~= nil and sent_setting("zoom_overlap_h") == nil)
for _, item in ipairs(profiles:getSubMenuItems()) do
    if item.name == "PDF copy" then commands = item.sub_item_table; break end
end
choose(commands, "Delete").callback(menu)
assert(profiles.data["PDF copy"] and visible.ok_text == "Delete")
visible.ok_callback()
assert(profiles.data["PDF copy"] == nil)
print("Stock factory, reflow/PDF settings, action editor, rename/duplicate/delete, format gate and batched execution verified")
