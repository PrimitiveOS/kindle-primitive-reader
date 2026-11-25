-- Run in the pinned native Mac runtime with a scratch KO_HOME.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
G_reader_settings = require("luasettings"):wrap({})
local Device = require("device")
require("document/canvascontext"):init(Device)
local ReaderFooter = require("apps/reader/modules/readerfooter")
local TextWidget = require("ui/widget/textwidget")
local Font = require("ui/font")
local UIManager = require("ui/uimanager")

local output = assert(arg[1])
local generators
for index = 1, 20 do
    local name, value = debug.getupvalue(ReaderFooter.updateFooterTextGenerator, index)
    if name == "footerTextGeneratorMap" then
        generators = value
        break
    end
end
assert(generators, "Missing the pinned footer generators")

local refreshes, menu_updates = 0, 0
local footer = setmetatable({
    pageno = 2,
    settings = setmetatable({ text_font_bold = true }, { __index = ReaderFooter.default_settings }),
    _saved_screen_width = Device.screen:getWidth(),
    footer_text_face = Font:getFace("cfont", 14),
    refreshFooter = function(self, render, recompute)
        assert(render and recompute)
        refreshes = refreshes + 1
    end,
    ui = {
        doc_props = {
            display_title = "A general outline of tide, wind, and coastal navigation",
            authors = "Nell Harper and the Harbor Historical Society",
        },
        toc = { getTocTitleByPage = function(_, page)
            assert(page == 2)
            return "Chapter two. Charts and bearings from the north shore"
        end },
    },
}, { __index = ReaderFooter })
local menu = { updateItems = function() menu_updates = menu_updates + 1 end }
local function shown()
    return UIManager._window_stack[#UIManager._window_stack].widget
end
local function buttons(spin)
    return spin.vgroup[3][1].buttons
end
local function apply(spin)
    local definitions = buttons(spin)
    definitions[#definitions][2].callback()
end
local function width(text)
    local widget = TextWidget:new{ text = text, face = footer.footer_text_face, bold = true }
    local actual = widget:getSize().w
    widget:free()
    return actual
end
local function normalized(text)
    return (text:gsub("\226\129[\166-\169]", ""):gsub("\194\160", " "))
end

local rows = {
    {"Book-author item", "book_author_max_width_pct", generators.book_author},
    {"Book-title item", "book_title_max_width_pct", generators.book_title},
    {"Chapter-title item", "book_chapter_max_width_pct", generators.book_chapter},
}
for i, row in ipairs(rows) do
    local label, key, generate = unpack(row)
    assert(footer.settings[key] == 30)
    local item = footer:genItemMaxWidthMenuItems(label, label .. ": %1%", key)
    item.callback(menu)
    local spin = shown()
    assert(spin.value_min == 10 and spin.value_max == 100)
    assert(spin.value == 30 and spin.default_value == 30 and spin.unit == "%")
    assert(spin.value_step == 5 and spin.value_hold_step == 20 and spin.keep_shown_on_apply)
    if i == 2 then
        UIManager:forceRePaint()
        Device.screen.bb:writePNG(output .. "/stock-footer-width-picker.png", false)
    end
    spin:onSpinButtonPressed({ 1, false })
    assert(spin.value_widget:getValue() == 35)
    spin:onSpinButtonPressed({ -1, true })
    assert(spin.value_widget:getValue() == 15)
    spin:onSpinButtonPressed({ -1, true })
    assert(spin.value_widget:getValue() == 10 and footer.settings[key] == 30)
    apply(spin)
    assert(footer.settings[key] == 10 and shown() == spin)
    local narrow = normalized(generate(footer))
    assert(narrow:sub(-3) == "…" and width(narrow) <= footer._saved_screen_width / 10)
    for k = 1, 5 do spin:onSpinButtonPressed({ 1, true }) end
    assert(spin.value_widget:getValue() == 100)
    apply(spin)
    local wide = normalized(generate(footer))
    assert(wide:sub(-3) ~= "…" and width(wide) > width(narrow))
    buttons(spin)[1][1].callback()
    assert(spin.value_widget:getValue() == 30 and footer.settings[key] == 100)
    spin.value_widget.text_value.callback()
    local entry = shown()
    assert(entry ~= spin)
    entry.getInputText = function() return "47" end
    entry.buttons[1][2].callback()
    assert(shown() == spin and spin.value_widget:getValue() == 47)
    assert(footer.settings[key] == 100)
    apply(spin)
    assert(footer.settings[key] == 47)
    UIManager:close(spin)
end
assert(refreshes == 9 and menu_updates == 9)
print("Stock footer width spinners and measured ellipses verified for all three independent values")
