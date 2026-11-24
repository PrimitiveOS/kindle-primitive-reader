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
    tableRemoveValue = function(values, ...)
        local keys = {...}
        for i = 1, #keys - 1 do
            values = values[keys[i]]
            if not values then return end
        end
        values[keys[#keys]] = nil
    end,
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
local function commands_for(name)
    for _, item in ipairs(profiles:getSubMenuItems()) do
        if item.name == name then return item.sub_item_table end
    end
    error("Missing profile " .. name)
end
local function value_action(commands, category, label)
    local edit
    for _, item in ipairs(commands) do
        if item.text_func and item.text_func():match("Edit actions") then
            edit = item
            break
        end
    end
    assert(edit)
    local section = choose(edit.sub_item_table_func(), category)
    for _, item in ipairs(section.sub_item_table) do
        if item.text_func then
            local ok, text = pcall(item.text_func)
            if ok and text and text:lower():sub(1, #label) == label then
                return item
            end
        end
    end
    error("Missing editable value " .. label)
end
local function set_number(action, menu_instance, initial, minimum, maximum, default, value, step)
    assert(action.checked_func())
    action.callback(menu_instance)
    assert(visible.value == initial and visible.default_value == default)
    assert(visible.value_min == minimum and visible.value_max == maximum)
    assert(visible.value_hold_step == 5)
    if step then assert(visible.value_step == step) end
    visible.callback({value = value})
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

local text_live = profiles.document.configurable
local text_commands = commands_for("Text")
local section = "Reflowable documents (epub, fb2, txt…)"
local live_spacing = text_live.line_spacing
local live_font = text_live.font_size
local live_dpi = text_live.render_dpi
set_number(value_action(text_commands, section, "line spacing"), menu, live_spacing, 50, 200, 100, 137)
set_number(value_action(text_commands, section, "font size"), menu, live_font, 12, 44, 22, 31.5, 0.5)
assert(text.line_spacing == 137 and text.font_size == 31.5)
assert(text_live.line_spacing == live_spacing and text_live.font_size == live_font)

local zoom = value_action(text_commands, section, "zoom")
local zoom_choices = zoom.sub_item_table_func()
local expected_dpi = {0, 48, 96, 167, 212, 300}
assert(#zoom_choices == #expected_dpi)
assert(zoom_choices[3].checked_func())
for index, dpi in ipairs(expected_dpi) do
    zoom_choices[index].callback()
    assert(text.render_dpi == dpi and zoom_choices[index].checked_func())
    assert(text_live.render_dpi == live_dpi)
end
zoom_choices[4].callback()
assert(text.render_dpi == 167)

local live_word_spacing = text_live.word_spacing
local live_word_expansion = text_live.word_expansion
local word_spacing = value_action(text_commands, section, "word spacing").sub_item_table_func()
local word_expansion = value_action(text_commands, section, "word expansion").sub_item_table_func()
local stock_spacing = {{75, 50}, {95, 75}, {100, 90}}
local stock_expansion = {0, 5, 15}
assert(#word_spacing == #stock_spacing and word_spacing[2].checked_func())
assert(#word_expansion == #stock_expansion and word_expansion[1].checked_func())
for i, pair in ipairs(stock_spacing) do
    word_spacing[i].callback()
    assert(word_spacing[i].checked_func())
    assert(text.word_spacing[1] == pair[1] and text.word_spacing[2] == pair[2])
    word_expansion[i].callback()
    assert(word_expansion[i].checked_func() and text.word_expansion == stock_expansion[i])
end
assert(text_live.word_spacing == live_word_spacing)
assert(text_live.word_expansion == live_word_expansion)
local spacing_config
local expansion_config
for _, tab in ipairs(require("ui/data/creoptions")) do
    for _, option in ipairs(tab.options) do
        if option.name == "word_spacing" then spacing_config = option.more_options_param end
        if option.name == "word_expansion" then expansion_config = option.more_options_param end
    end
end
assert(spacing_config.left_min == 10 and spacing_config.left_max == 500)
assert(spacing_config.right_min == 25 and spacing_config.right_max == 100)
assert(spacing_config.left_hold_step == 10 and spacing_config.right_hold_step == 10)
assert(expansion_config.value_min == 0 and expansion_config.value_max == 20)
assert(expansion_config.value_step == 1 and expansion_config.value_hold_step == 4)

local live_gamma = text_live.font_gamma
local live_weight = text_live.font_base_weight
local gamma_choices = value_action(text_commands, section, "contrast").sub_item_table_func()
local weight_choices = value_action(text_commands, section, "font weight").sub_item_table_func()
local stock_gamma = {10, 15, 25, 30, 36, 43, 49, 56}
local stock_weight = {-1, -0.5, 0, 0.5, 1, 1.5, 3}
assert(#gamma_choices == #stock_gamma and gamma_choices[2].checked_func())
assert(#weight_choices == #stock_weight and weight_choices[3].checked_func())
for i, level in ipairs(stock_gamma) do
    gamma_choices[i].callback()
    assert(text.font_gamma == level and gamma_choices[i].checked_func())
end
for i, adjustment in ipairs(stock_weight) do
    weight_choices[i].callback()
    assert(text.font_base_weight == adjustment and weight_choices[i].checked_func())
end
gamma_choices[3].callback()
weight_choices[5].callback()
assert(text.font_gamma == 25 and text.font_base_weight == 1)
assert(text_live.font_gamma == live_gamma and text_live.font_base_weight == live_weight)

local live_sides = text_live.h_page_margins
local live_top = text_live.t_page_margin
local live_bottom = text_live.b_page_margin
local live_link = text_live.sync_t_b_page_margins
local sides = value_action(text_commands, section, "l/r margins").sub_item_table_func()
local margin_choices = {5, 10, 15, 20, 30, 50, 70, 100, 140}
assert(#sides == #margin_choices)
for i, value in ipairs(margin_choices) do
    sides[i].callback()
    assert(text.h_page_margins[1] == value and text.h_page_margins[2] == value)
end
sides[4].callback()
set_number(value_action(text_commands, section, "top margin"), menu, live_top, 0, 140, 15, 83)
set_number(value_action(text_commands, section, "bottom margin"), menu, live_bottom, 0, 140, 15, 42)
assert(text.t_page_margin == 83 and text.b_page_margin == 42)
local link = value_action(text_commands, section, "sync t/b margins").sub_item_table_func()
assert(#link == 2 and link[1].checked_func())
link[2].callback()
assert(text.sync_t_b_page_margins == true and link[2].checked_func())
link[1].callback()
assert(text.sync_t_b_page_margins == false and link[1].checked_func())
assert(text_live.h_page_margins == live_sides)
assert(text_live.t_page_margin == live_top and text_live.b_page_margin == live_bottom)
assert(text_live.sync_t_b_page_margins == live_link)

profiles.ui.rolling = nil
profiles.document = document("ui/data/koptoptions")
profiles.document.configurable.zoom_overlap_h = 38
root_menu[2].callback(menu)
stock_name(nil, "PDF")
local pdf = profiles.data.PDF
assert(pdf.kopt_zoom_overlap_h == 38 and pdf.set_font == nil and pdf.font_size == nil)
assert(pdf.kopt_page_scroll ~= nil and pdf.kopt_zoom_mode_genus ~= nil)
local commands = commands_for("PDF")
local fixed_section = "Fixed layout documents (pdf, djvu, pics…)"
local horizontal = value_action(commands, fixed_section, "horizontal overlap")
local vertical = value_action(commands, fixed_section, "vertical overlap")
local pdf_live_horizontal = profiles.document.configurable.zoom_overlap_h
local pdf_live_vertical = profiles.document.configurable.zoom_overlap_v
set_number(horizontal, menu, pdf_live_horizontal, 0, 84, 36, 45)
set_number(vertical, menu, pdf_live_vertical, 0, 84, 36, 57)
assert(pdf.kopt_zoom_overlap_h == 45 and pdf.kopt_zoom_overlap_v == 57)
assert(profiles.document.configurable.zoom_overlap_h == pdf_live_horizontal)
assert(profiles.document.configurable.zoom_overlap_v == pdf_live_vertical)
choose(commands, "Duplicate").callback(menu)
stock_name("PDF", "PDF copy")
assert(profiles.data["PDF copy"].kopt_zoom_overlap_h == 45)
-- Holding an action in the stock editor removes both its value and ordered step.
horizontal.hold_callback(menu)
assert(pdf.kopt_zoom_overlap_h == nil)
for _, key in ipairs(pdf.settings.order) do assert(key ~= "kopt_zoom_overlap_h") end
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
profiles:onProfileExecute("PDF copy", {qm_show = false})
assert(events[1].name == "BatchedUpdate" and events[#events].name == "BatchedUpdateDone")
assert(sent_setting("zoom_overlap_h") == 45 and sent_setting("zoom_overlap_v") == 57)
assert(sent_setting("embedded_css") == nil and sent_setting("line_spacing") == pdf.kopt_line_spacing)
events = {}
profiles:onProfileExecute("Scanned pages", {qm_show = false})
assert(sent_setting("zoom_overlap_h") == nil and sent_setting("zoom_overlap_v") == 57)
events = {}
profiles:onProfileExecute("Text", {qm_show = false})
assert(sent_setting("embedded_css") == nil and sent_setting("line_spacing") == nil)
assert(sent_setting("font_size") == nil and sent_setting("render_dpi") == nil)
assert(sent_setting("font_gamma") == nil and sent_setting("font_base_weight") == nil)
assert(sent_setting("word_spacing") == nil and sent_setting("word_expansion") == nil)
profile_ui.instance = {rolling = true}
events = {}
profiles:onProfileExecute("Text", {qm_show = false})
assert(sent_setting("embedded_css") ~= nil and sent_setting("zoom_overlap_h") == nil)
assert(sent_setting("line_spacing") == 137 and sent_setting("zoom_overlap_v") == nil)
assert(sent_setting("font_size") == 31.5 and sent_setting("render_dpi") == 167)
assert(sent_setting("font_gamma") == 25 and sent_setting("font_base_weight") == 1)
local spaces = sent_setting("word_spacing")
assert(spaces[1] == 100 and spaces[2] == 90 and sent_setting("word_expansion") == 15)
local applied_margins = sent_setting("h_page_margins")
assert(applied_margins[1] == 20 and applied_margins[2] == 20)
assert(sent_setting("t_page_margin") == 83 and sent_setting("b_page_margin") == 42)
assert(sent_setting("sync_t_b_page_margins") == 0)
for _, item in ipairs(profiles:getSubMenuItems()) do
    if item.name == "PDF copy" then commands = item.sub_item_table; break end
end
choose(commands, "Delete").callback(menu)
assert(profiles.data["PDF copy"] and visible.ok_text == "Delete")
visible.ok_callback()
assert(profiles.data["PDF copy"] == nil)
print("Stock saved font rendering verified: eight indexed gamma and seven weight presets, with document isolation and reflow dispatch.")
print("Stock saved word spacing pairs and expansion presets verified, with exact reader options 10–500 / 25–100 and 0–20.")
print("Stock saved margins: nine L/R pair presets 5–140, T/B exact 0–140 with default 15 and held step 5, and separate link flag.")
print("Stock saved font size: 12–44, default 22, step 0.5, held step 5; content zoom presets 0/48/96/167/212/300 dpi.")
print("Stock saved line spacing: 50–200%, default 100%, held step 5; profile-only change dispatches 137%.")
print("Stock saved horizontal and vertical overlap: 0–84%, default 36%, held step 5; profile-only values dispatch independently.")
print("Stock factory, action removal, rename/duplicate/delete, format gate and batched execution verified.")
