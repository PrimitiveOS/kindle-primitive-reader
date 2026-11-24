-- Run with LuaJIT and the pinned KOReader source tree. No writes or live device services.
local root = assert(arg[1], "Pass the pinned KOReader source tree")
local native_util
if arg[2] == "native" then
    require("setupkoenv")
    native_util = require("util")
    package.loaded.util = nil
end
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
    gsplit = native_util and native_util.gsplit or function(text, separator)
        assert(separator == ",")
        return text:gmatch("[^,]+")
    end,
    trim = native_util and native_util.trim or function(value) return value:match("^%s*(.-)%s*$") end,
    stringSearch = native_util and native_util.stringSearch or function(value, term)
        return (value or ""):lower():find(term:lower(), 1, true) or 0
    end,
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
    tableSetValue = function(values, value, ...)
        local keys = {...}
        for i = 1, #keys - 1 do
            values[keys[i]] = values[keys[i]] or {}
            values = values[keys[i]]
        end
        values[keys[#keys]] = value
    end,
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
local next_tick = {}
local after_next_tick = {}
local scheduled = {}
local visible
mock("ui/uimanager", {
    sendEvent = function(_, event) events[#events + 1] = event end,
    broadcastEvent = function(_, event) events[#events + 1] = event end,
    setSilentMode = function() end,
    show = function(_, widget) visible = widget end,
    close = function() visible = nil end,
    nextTick = function(_, callback) table.insert(next_tick, callback) end,
    tickAfterNext = function(_, callback) table.insert(after_next_tick, callback) end,
    scheduleIn = function(_, seconds, callback) table.insert(scheduled, {seconds, callback}) end,
})
for _, name in ipairs{"confirmbox", "inputdialog", "infomessage", "datetimewidget", "spinwidget"} do
    mock("ui/widget/" .. name, {new = function(_, widget)
        widget.getInputText = function(self) return self.input end
        widget.addTextToInput = function(self, text) self.input = (self.input or "") .. text end
        widget.onShowKeyboard = function() end
        return widget
    end})
end
mock("ui/widget/container/widgetcontainer", {extend = function(_, options) return options end})
for _, name in ipairs{"apps/filemanager/filemanager", "apps/reader/modules/readerdictionary",
    "apps/reader/modules/readerfooter", "apps/reader/modules/readerhighlight",
    "apps/reader/modules/readertypography", "apps/reader/modules/readerview",
    "apps/reader/modules/readerzooming", "device/key", "luasettings", "ui/size", "fontlist"} do
    mock(name, {})
end
mock("logger", {dbg = function() end})
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

for _, setting in ipairs({
    {"font hinting", "font_hinting", {0, 1, 2}, 3, 2},
    {"font kerning", "font_kerning", {0, 1, 2, 3}, 4, 3},
    {"embedded style", "embedded_css", {false, true}, 2, 1},
    {"embedded fonts", "embedded_fonts", {false, true}, 2, 1},
    {"image scaling", "smooth_scaling", {false, true}, 1, 2},
    {"invert images", "nightmode_images", {false, true}, 2, 1},
}) do
    local label, key, options, default, final = unpack(setting)
    local live = text_live[key]
    local choices = value_action(text_commands, section, label).sub_item_table_func()
    assert(#choices == #options and choices[default].checked_func())
    for i, option in ipairs(options) do
        choices[i].callback()
        assert(text[key] == option and choices[i].checked_func())
    end
    choices[final].callback()
    assert(text[key] == options[final] and text_live[key] == live)
end

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
local pdf_live_margin = profiles.document.configurable.page_margin
local pdf_live_contrast = profiles.document.configurable.contrast
assert(pdf.kopt_page_margin == 0.10 and pdf.kopt_contrast == 1.0)
assert(pdf.kopt_white_threshold == nil)
set_number(horizontal, menu, pdf_live_horizontal, 0, 84, 36, 45)
set_number(vertical, menu, pdf_live_vertical, 0, 84, 36, 57)
assert(pdf.kopt_zoom_overlap_h == 45 and pdf.kopt_zoom_overlap_v == 57)
assert(profiles.document.configurable.zoom_overlap_h == pdf_live_horizontal)
assert(profiles.document.configurable.zoom_overlap_v == pdf_live_vertical)
local margin_choices = value_action(commands, fixed_section, "margin").sub_item_table_func()
local gamma_choices = value_action(commands, fixed_section, "contrast").sub_item_table_func()
local margins = {0.05, 0.10, 0.25, 0.40, 0.55, 0.70, 0.85, 1.00}
local gammas = {0.8, 1.0, 1.5, 2.0, 4.0, 6.0, 10.0, 50.0}
assert(#margin_choices == #margins and margin_choices[2].checked_func())
assert(#gamma_choices == #gammas and gamma_choices[2].checked_func())
for i = 1, #margins do
    margin_choices[i].callback()
    gamma_choices[i].callback()
    assert(pdf.kopt_page_margin == margins[i] and margin_choices[i].checked_func())
    assert(pdf.kopt_contrast == gammas[i] and gamma_choices[i].checked_func())
end
margin_choices[5].callback()
gamma_choices[4].callback()
assert(profiles.document.configurable.page_margin == pdf_live_margin)
assert(profiles.document.configurable.contrast == pdf_live_contrast)
for _, row in ipairs(require("ui/data/koptoptions")) do
    for _, opt in ipairs(row.options) do
        if opt.name == "page_margin" then
            local more = opt.more_options_param
            assert(more.value_min == 0 and more.value_max == 1.50)
            assert(more.value_step == 0.01 and more.value_hold_step == 0.10)
        elseif opt.name == "contrast" then
            local more = opt.more_options_param
            assert(more.value_min == 0.8 and more.value_max == 50)
            assert(more.value_step == 0.1 and more.value_hold_step == 1)
        end
    end
end
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
assert(sent_setting("page_margin") == 0.55 and sent_setting("contrast") == 2.0)
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
assert(sent_setting("page_margin") == nil and sent_setting("contrast") == nil)
assert(sent_setting("line_spacing") == 137 and sent_setting("zoom_overlap_v") == nil)
assert(sent_setting("font_size") == 31.5 and sent_setting("render_dpi") == 167)
assert(sent_setting("font_gamma") == 25 and sent_setting("font_base_weight") == 1)
assert(sent_setting("font_hinting") == 1 and sent_setting("font_kerning") == 2)
assert(sent_setting("embedded_css") == 0 and sent_setting("embedded_fonts") == 0)
assert(sent_setting("smooth_scaling") == 1 and sent_setting("nightmode_images") == 0)
local spaces = sent_setting("word_spacing")
assert(spaces[1] == 100 and spaces[2] == 90 and sent_setting("word_expansion") == 15)
local applied_margins = sent_setting("h_page_margins")
assert(applied_margins[1] == 20 and applied_margins[2] == 20)
assert(sent_setting("t_page_margin") == 83 and sent_setting("b_page_margin") == 42)
assert(sent_setting("sync_t_b_page_margins") == 0)

-- Exercise the actual stock book-opening submenu, ReaderReady event and
-- deferred format-gated execution on both a text and a fixed-page document.
local function always_on_open(name)
    local section = profiles:genAutoExecDocConditionalMenuItem("on book opening", "ReaderReadyAll", name)
    local always = section.sub_item_table_func()[1]
    assert(always.text == "always" and not section.checked_func())
    return section, always
end
local text_open, always_text = always_on_open("Text")
local pdf_open, always_pdf = always_on_open("Scanned pages")
always_text.callback()
always_pdf.callback()
assert(text_open.checked_func() and pdf_open.checked_func())
local function trigger_open()
    events, next_tick = {}, {}
    profiles:onReaderReady()
    assert(#events == 0 and #next_tick == 2)
    for _, callback in ipairs(next_tick) do callback() end
end
trigger_open()
assert(sent_setting("font_size") == 31.5 and sent_setting("zoom_overlap_v") == nil)
profile_ui.instance = {paging = true}
trigger_open()
assert(sent_setting("zoom_overlap_v") == 57 and sent_setting("render_dpi") == nil)
profiles.ui.reloading = true
next_tick = {}
profiles:onReaderReady()
assert(#next_tick == 0)
profiles.ui.reloading = false
always_text.callback()
always_pdf.callback()
profiles:onReaderReady()
assert(not text_open.checked_func() and not pdf_open.checked_func() and #next_tick == 0)
profile_ui.instance = {rolling = true}

mock("readcollection", {
    isFileInCollection = function(_, file, name)
        return name == "Sea surveys" and file == "/documents/enrolled.epub"
    end,
})
local folder_settings = { lastdir = "/documents/Downloads" }
G_reader_settings = { readSetting = function(_, key, default) return folder_settings[key] or default end }
for _, widget in ipairs({"buttondialog", "menu", "pathchooser"}) do
    mock("ui/widget/" .. widget, {})
end
mock("ui/widget/screenshoter", { default_dir = "/app-data/screenshots" })
mock("ui/widget/dictquicklookup", { getWikiSaveEpubDefaultDir = function() return "/documents/Wikipedia" end })
mock("apps/filemanager/filemanagerutil", { getHomeFolder = function() return "/documents" end })
mock("libs/libkoreader-lfs", {})
local folders = dofile(root .. "/frontend/apps/filemanager/filemanagershortcuts.lua")
profiles.ui.folder_shortcuts = folders
profiles.ui.collections = {
    onShowCollList = function(_, _, callback) callback({ ["Sea surveys"] = true }) end,
}
profiles.document.file = "/documents/ordinary.epub"
local rotation = 0
screen.getRotationMode = function() return rotation end
local rules = text_open.sub_item_table_func()
rules[2].sub_item_table_func()[2].callback() -- clockwise
rules[4].callback(menu)
visible.input = "waypoint, ridge"
choose(visible.buttons[#visible.buttons], "Save").callback()
rules[5].callback(menu)
assert(rules[2].checked_func() and rules[4].checked_func() and rules[5].checked_func())
local function conditional_open(file, turn, new_book, matches)
    profiles.document.file, profiles.document.is_new = file, new_book
    rotation = turn
    events, next_tick = {}, {}
    profiles:onReaderReady()
    assert(#next_tick == (matches and 1 or 0))
    for _, callback in ipairs(next_tick) do callback() end
    assert(sent_setting("font_size") == (matches and 31.5 or nil))
end
conditional_open("/documents/ordinary.epub", 0, true, false)
conditional_open("/documents/ordinary.epub", 1, false, true)
conditional_open("/documents/wayPOINT.epub", 0, false, true)
conditional_open("/documents/enrolled.epub", 0, false, true)
rules[5].hold_callback(menu) -- keep the path oracle independent of collection matches
rules[2].sub_item_table_func()[2].callback()
local function save_path(phrase, preview)
    rules[4].callback(menu)
    visible.input = phrase
    if preview then
        local dialog = visible
        choose(dialog.buttons[#dialog.buttons - 1], "Show").callback()
        assert(visible.text == preview and dialog.input == phrase)
        visible = dialog
    end
    choose(visible.buttons[#visible.buttons], "Save").callback()
end
rules[4].callback(menu)
choose(visible.buttons[#visible.buttons - 1], "Info").callback()
assert(visible.text:find("%H% Home", 1, true) and visible.text:find("%S% Screenshot folder", 1, true))
save_path("%H%/Survey, %D%/Loan", "/documents/Survey, /documents/Downloads/Loan")
conditional_open("/documents/SURVEY/paper.epub", 0, false, true)
conditional_open("/documents/Downloads/Loan/paper.epub", 0, false, true)
conditional_open("/outside/Survey/paper.epub", 0, false, false)
assert(rules[4].text_func():find("/documents/Downloads/Loan", 1, true))
assert(folders:expandPath("%H%%D%") == "/documents/documents/Downloads")
assert(folders:expandPath("%H% %S% %W% %h% %9% %Z%") == "/documents /app-data/screenshots /documents/Wikipedia %h% %9% %Z%")
assert(folders:expandPath("%Z%H%") == "%Z%H%")
assert(folders:expandPath("%%H%%") == "%/documents%")
save_path('"%H%/Survey", %Z%', '"/documents/Survey", %Z%')
conditional_open("/documents/Survey/paper.epub", 0, false, false) -- quotes are literal
conditional_open('/library/"/documents/Survey".epub', 0, false, true)
conditional_open("/documents/%z%.epub", 0, false, true) -- unknown variables remain literal
folder_settings.lastdir = nil
assert(folders:expandPath("%D%") == "%D%")
folder_settings.download_dir = "/documents/Charts, atlas"
save_path("%D%/Loan", "/documents/Charts, atlas/Loan")
conditional_open("/documents/Charts/report.epub", 0, false, true)
conditional_open("/elsewhere/atlas/Loan/report.epub", 0, false, true)
folder_settings.download_dir, folder_settings.lastdir = nil, "/documents/Downloads"
save_path("waypoint, ridge")
rules[2].sub_item_table_func()[2].callback()
rules[6].callback(menu)
conditional_open("/documents/wayPOINT.epub", 1, false, false)
conditional_open("/documents/wayPOINT.epub", 1, true, true)
always_text.callback()
assert(text_open.checked_func() and not rules[2].enabled_func())
assert(not require("util").tableGetValue(profiles.autoexec, "ReaderReadyAll", "Text"))
always_text.callback()

-- Exercise stock metadata rules through the menu's actual buttons and ReaderReady.
local property_names = { "title", "authors", "series", "series_index", "language", "keywords", "description" }
local property_text = {
    title = "Title:", authors = "Authors:", series = "Series:", series_index = "Series index:",
    language = "Language:", keywords = "Keywords:", description = "Description:",
}
profiles.ui.bookinfo = { props = property_names, prop_text = property_text }
profiles.ui.doc_props = {
    title = "Raw title", display_title = "ĐẶNG [A+B]", authors = "Ada Scout", series = "Sounding Log",
    series_index = "2.5", language = "vi", keywords = "Estuary, Circulation", description = "Lower river delta",
}
local metadata = rules[3].sub_item_table_func()
local function set_metadata(index, pattern)
    metadata[index].callback(menu)
    visible.input = pattern
    choose(visible.buttons[#visible.buttons], "Save").callback()
end
metadata[1].callback(menu)
assert(visible.buttons[1][1].enabled_func())
visible.buttons[1][1].callback()
assert(visible.input == "ĐẶNG [A+B]")
for index, phrase in ipairs({"none, [a+b]", "ADA SCOUT", "sounding", "2.5", "VI", "circulation", "river DELTA"}) do
    set_metadata(index, phrase)
    assert(metadata[index].checked_func())
    conditional_open("/documents/ordinary.epub", 0, false, true)
    metadata[index].hold_callback(menu)
    assert(not metadata[index].checked_func())
end
set_metadata(1, "raw title")
conditional_open("/documents/ordinary.epub", 0, false, false)
set_metadata(1, native_util and "đặng [a+b]" or "[a+b]")
set_metadata(2, "Scout")
conditional_open("/documents/ordinary.epub", 0, false, true) -- two matching fields, one execution
set_metadata(1, "")
profiles.ui.doc_props.authors = nil
conditional_open("/documents/ordinary.epub", 0, false, false)
metadata[2].callback(menu)
assert(not visible.buttons[1][1].enabled_func())
choose(visible.buttons[#visible.buttons], "Cancel").callback()
profiles.ui.doc_props.authors = "Ada Scout"
rules[6].callback(menu)
conditional_open("/documents/ordinary.epub", 0, false, false)
conditional_open("/documents/ordinary.epub", 0, true, true)
if native_util then
    assert(native_util.stringSearch(nil, "") == 1)
    assert(not pcall(native_util.stringSearch, 2.5, "2.5"))
end
always_text.callback()
assert(not require("util").tableGetValue(profiles.autoexec, "ReaderReadyAll", "Text"))
always_text.callback()

if arg[3] then
    -- Only load the authored output of koreader-custom-metadata-check.lua.
    profiles.ui.doc_props = dofile(arg[3])
    set_metadata(1, "coastal ledger")
    conditional_open("/documents/Field Notes.epub", 0, false, true)
    set_metadata(1, "publisher title, stale title")
    conditional_open("/documents/Field Notes.epub", 0, false, false)
    set_metadata(5, "en")
    conditional_open("/documents/Field Notes.epub", 0, false, true)
    metadata[1].callback(menu)
    visible.input = ""
    visible.buttons[1][1].callback()
    assert(visible.input == "Coastal Ledger")
    choose(visible.buttons[#visible.buttons], "Cancel").callback()
    always_text.callback()
    always_text.callback()
    print("Stock Profiles matched effective custom metadata and live publisher fallback, with Current book.")
end

-- Drive the stock time editor and event at each minute through a full day.
profiles.external_autoexec_triggers = {}
local interval = profiles:genAutoExecMenu("Text", profiles.data.Text)[3]
local endpoints = interval.sub_item_table_func()
local real_date, now = os.date, "12:34"
os.date = function(format, ...)
    if format == "%H:%M" then return now end
    return real_date(format, ...)
end
local function set_endpoint(index, hour, min)
    endpoints[index].callback(menu)
    assert(visible.ok_text == "Set time")
    visible.callback({hour = hour, min = min})
end
assert(not interval.checked_func())
endpoints[1].callback(menu)
assert(visible.hour == 12 and visible.min == 34)
set_endpoint(1, 6, 0)
assert(interval.checked_func() and endpoints[2].text_func() == "end: 06:00")
always_text.callback()
local function check_day(expected)
    for minute = 0, 1439 do
        now = string.format("%02d:%02d", math.floor(minute / 60), minute % 60)
        conditional_open("/documents/ordinary.epub", 0, false, expected(minute))
    end
end
check_day(function() return true end) -- Equal start and end cover all day.
set_endpoint(2, 12, 30)
assert(interval.text_func() == "Only within time interval: 06:00 - 12:30")
check_day(function(time) return time >= 360 and time <= 750 end)
set_endpoint(1, 22, 0)
set_endpoint(2, 6, 30)
check_day(function(time) return time >= 1320 or time <= 390 end)
always_text.callback()
set_metadata(2, "Scout")
now = "21:59"
conditional_open("/documents/ordinary.epub", 0, true, false)
now = "22:00"
conditional_open("/documents/ordinary.epub", 0, true, true)
metadata[2].hold_callback(menu)
interval.hold_callback(menu)
assert(not interval.checked_func() and endpoints[1].text_func() == "start: --:--")
os.date = real_date

local execution = profiles:genAutoExecMenu("Text", profiles.data.Text)
local ask, delay = execution[1], execution[2]
local delays = delay.sub_item_table_func()
assert(not ask.checked_func() and delays[2].checked_func())
assert(ask.text == "Ask to execute" and delay.text_func() == "Executing delay: default")
ask.callback()
always_text.callback()
local function open_with_confirmation()
    events, next_tick, scheduled, visible = {}, {}, {}, nil
    profiles:onReaderReady()
    assert(visible and visible.ok_text == "Execute" and visible.text:find("Text", 1, true))
    assert(#events == 0 and #next_tick == 0 and #scheduled == 0)
end
open_with_confirmation()
visible = nil -- Dismiss without calling Execute.
assert(#events == 0 and #next_tick == 0 and #scheduled == 0)
open_with_confirmation()
visible.ok_callback()
assert(#next_tick == 1 and #events == 0 and #scheduled == 0)
next_tick[1]()
assert(sent_setting("font_size") == 31.5)

delays[1].callback()
open_with_confirmation()
visible.ok_callback()
assert(sent_setting("font_size") == 31.5 and #next_tick == 0 and #scheduled == 0)
delays[3].callback(menu)
assert(visible.value == 3 and visible.value_min == 1 and visible.value_max == 10)
assert(visible.value_hold_step == 0.1 and visible.precision == "%0.1f")
visible.callback({value = 3.4})
assert(delays[3].checked_func() and not delays[1].checked_func())
profiles.data.Text.settings.auto_exec_time_interval = {"06:00", "06:01"}
os.date = function(format, ...) return format == "%H:%M" and now or real_date(format, ...) end
now = "06:00"
open_with_confirmation()
now = "06:02"
visible.ok_callback()
assert(#events == 0 and #next_tick == 0 and #scheduled == 1 and scheduled[1][1] == 3.4)
scheduled[1][2]()
assert(sent_setting("font_size") == 31.5)
os.date = real_date
profiles.data.Text.settings.auto_exec_time_interval = nil
delay.hold_callback(menu)
assert(delays[2].checked_func())
ask.callback()
always_text.callback()

-- CloseDocument fires before ReaderUI releases its instance. Replay the stock
-- callback in that context, then advance to file-manager context after closing.
local close_menu = profiles:genAutoExecDocConditionalMenuItem("on book closing", "CloseDocumentAll", "Text")
local always_close = close_menu.sub_item_table_func()[1]
always_close.callback()
assert(close_menu.checked_func())
local function close_book(fixed)
    profile_ui.instance = fixed and {paging = true} or {rolling = true}
    events, next_tick, after_next_tick, scheduled, visible = {}, {}, {}, {}, nil
    profiles:onCloseDocument()
end
delays[1].callback()
close_book()
assert(sent_setting("font_size") == 31.5 and #after_next_tick == 0)
profiles.ui.reloading = true
close_book()
assert(#events == 0)
profiles.ui.reloading = nil

delays[2].callback()
close_book()
assert(#events == 0 and #next_tick == 0 and #after_next_tick == 1)
profile_ui.instance = nil
after_next_tick[1]()
assert(sent_setting("font_size") == nil)

delays[3].callback(menu)
visible.callback({value = 3.4})
close_book()
assert(#events == 0 and #scheduled == 1 and scheduled[1][1] == 3.4)
profile_ui.instance = nil
scheduled[1][2]()
assert(sent_setting("font_size") == nil)

delays[1].callback()
ask.callback()
close_book()
assert(#events == 0 and visible and visible.ok_text == "Execute")
profile_ui.instance = nil
visible.ok_callback()
assert(sent_setting("font_size") == nil)
close_book()
visible.ok_callback()
assert(sent_setting("font_size") == 31.5)
ask.callback()
always_close.callback()
assert(not close_menu.checked_func())
delay.hold_callback(menu)

-- The shared close menu has four alternatives and omits opening's new-book gate.
local close_rules = close_menu.sub_item_table_func()
assert(#close_rules == 5 and close_rules[6] == nil and rules[6] ~= nil)
local pdf_close = profiles:genAutoExecDocConditionalMenuItem("on book closing", "CloseDocumentAll", "Scanned pages")
local pdf_close_rules = pdf_close.sub_item_table_func()
local function closing_path(section, pattern)
    section[4].callback(menu)
    visible.input = pattern
    choose(visible.buttons[#visible.buttons], "Save").callback()
end
delays[1].callback()
profiles:genAutoExecMenu("Scanned pages", profiles.data["Scanned pages"])[2].sub_item_table_func()[1].callback()
close_rules[2].sub_item_table_func()[2].callback()
close_rules[5].callback(menu)
close_rules[3].sub_item_table_func()[1].callback(menu)
visible.input = "coastal ledger"
choose(visible.buttons[#visible.buttons], "Save").callback()
closing_path(close_rules, "missing, %H%/Reports")
closing_path(pdf_close_rules, "%H%/Reports")
profiles.ui.doc_props.display_title = "Ordinary reading"
local function close_text_at(file, turn, expected, title)
    profiles.document.file, profiles.document.is_new = file, false
    profiles.ui.doc_props.display_title = title or "Ordinary reading"
    rotation = turn
    close_book(false)
    local changes = 0
    for _, event in ipairs(events) do
        if event.name == "ConfigChange" and event.args[1] == "font_size" then changes = changes + 1 end
    end
    assert(sent_setting("font_size") == (expected and 31.5 or nil))
    assert(changes == (expected and 1 or 0))
end
close_text_at("/documents/ordinary.epub", 0, false)
close_text_at("/documents/ordinary.epub", 1, true)
close_text_at("/documents/Reports/Log.epub", 0, true)
close_text_at("/documents/enrolled.epub", 0, true)
close_text_at("/documents/ordinary.epub", 0, true, "Coastal Ledger")
close_text_at("/documents/Reports/Log.epub", 1, true, "Coastal Ledger")
assert(sent_setting("zoom_overlap_v") == nil)
for _, file in ipairs({"/documents/Reports/Scan.pdf", "/documents/ordinary.pdf"}) do
    profiles.document.file = file
    rotation = 0
    close_book(true)
    assert(sent_setting("zoom_overlap_v") == (file:find("/Reports/", 1, true) and 57 or nil))
    assert(sent_setting("render_dpi") == nil)
end

profiles.data.Text.settings.auto_exec_time_interval = {"22:00", "06:30"}
os.date = function(format, ...) return format == "%H:%M" and now or real_date(format, ...) end
now = "21:59"
close_text_at("/documents/Reports/Log.epub", 1, false)
now = "22:00"
close_text_at("/documents/Reports/Log.epub", 1, true)
now = "06:30"
close_text_at("/documents/Reports/Log.epub", 1, true)
now = "06:31"
close_text_at("/documents/Reports/Log.epub", 1, false)
os.date = real_date
profiles.data.Text.settings.auto_exec_time_interval = nil
close_menu.hold_callback(menu)
pdf_close.hold_callback(menu)
assert(not close_menu.checked_func() and not pdf_close.checked_func())

for _, item in ipairs(profiles:getSubMenuItems()) do
    if item.name == "PDF copy" then commands = item.sub_item_table; break end
end
choose(commands, "Delete").callback(menu)
assert(profiles.data["PDF copy"] and visible.ok_text == "Delete")
visible.ok_callback()
assert(profiles.data["PDF copy"] == nil)
print("Stock saved font rendering verified: eight indexed gamma and seven weight presets, with document isolation and reflow dispatch.")
print("Stock saved hinting, kerning, embedded style/fonts and image switches verified, with reflow dispatch.")
print("Stock saved word spacing pairs and expansion presets verified, with exact reader options 10–500 / 25–100 and 0–20.")
print("Stock saved margins: nine L/R pair presets 5–140, T/B exact 0–140 with default 15 and held step 5, and separate link flag.")
print("Stock saved font size: 12–44, default 22, step 0.5, held step 5; content zoom presets 0/48/96/167/212/300 dpi.")
print("Stock saved line spacing: 50–200%, default 100%, held step 5; profile-only change dispatches 137%.")
print("Stock saved horizontal and vertical overlap: 0–84%, default 36%, held step 5; profile-only values dispatch independently.")
print("Stock saved scans: eight Margin and Contrast presets, isolated until Apply. Live exact range/steps match the stock option definitions.")
print("Stock factory, action removal, rename/duplicate/delete, format gate and batched execution verified.")
print("Stock book opening: Always toggle, ReaderReady dispatch, format gates, reload guard and disable verified.")
print("Stock conditional opening: orientation/path/collection alternatives, a separate new-book gate, and one execution per event.")
print("Stock path shortcuts: %H%/%D%/%S%/%W%, preview, single-pass expansion, literal quotes, and splitting after expansion.")
print("Stock metadata opening: seven fields, effective title, Current book, OR/only-new, and save/clear.")
print("Stock opening time: endpoint editor, every minute of daytime, overnight and equal-endpoint windows, conditional gating and clearing.")
print("Stock opening execution: confirmation before scheduling, Cancel, Prompt, next-tick Default and 1–10 second Custom.")
print("Stock closing execution: Always, reload guard, Prompt dispatch for active reader, and deferred/late confirmation format gating.")
print("Stock conditional closing: four OR alternatives, no new-book row, format gates and overnight time endpoints.")
