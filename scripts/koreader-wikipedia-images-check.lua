-- Run with the pinned macOS LuaJIT and the upstream KOReader source tree.
local root = assert(arg[1], "Pass the pinned KOReader source tree")
local fixture = assert(arg[2], "Pass the authored Wikimedia markup fixture")
local extract_path = assert(arg[3], "Pass the authored Wikipedia extract")
local file = assert(io.open(fixture))
local markup = file:read("*a")
file:close()
file = assert(io.open(extract_path))
local extract = file:read("*a")
file:close()

local values = {}
G_reader_settings = {
    nilOrTrue = function(_, key) return values[key] ~= false end,
    flipNilOrTrue = function(_, key) values[key] = values[key] == false end,
}
local function mock(name, value)
    package.preload[name] = function() return value end
end
mock("gettext", function(value) return value end)
_ = require("gettext")
mock("ffi/util", {template = function(value) return value end})
mock("device", {screen = {scaleBySize = function(_, size) return size end}})
mock("util", {
    htmlToPlainText = function(value)
        return (value:gsub("<[^>]->", ""):gsub("&amp;", "&"))
    end,
    isSplittable = function(character) return character == " " end,
})
mock("logger", {dbg = function() end, warn = function() end})
mock("socket.url", {unescape = function(value) return value end})
for _, name in ipairs{"json", "ui/renderimage", "libs/libkoreader-lfs", "ui/time"} do
    mock(name, {})
end

local f = assert(io.open(root .. "/frontend/apps/reader/modules/readerwikipedia.lua"))
local source = f:read("*a")
f:close()
local start = assert(source:find("                text = _(\"Show image in search results\")", 1, true))
local stop = assert(source:find("        }\n    }\nend", start, true))
local menu = assert(loadstring("return {{" .. source:sub(start, stop - 1) .. "}"))()
local lead_setting, more_setting = menu[1], menu[2]
assert(lead_setting.checked_func() and more_setting.enabled_func() and more_setting.checked_func())
more_setting.callback()
assert(not more_setting.checked_func())
lead_setting.callback()
assert(not lead_setting.checked_func() and not more_setting.enabled_func() and not more_setting.checked_func())
lead_setting.callback()
assert(more_setting.enabled_func() and not more_setting.checked_func())
more_setting.callback()

local wiki = dofile(root .. "/frontend/ui/wikipedia.lua")
local function page()
    return {
        title = "Harbor notebook",
        extract = extract,
        pageimage = "Harbor.png",
        thumbnail = {
            source = "https://upload.wikimedia.org/wikipedia/commons/thumb/4/44/Harbor.png/250px-Harbor.png",
            width = 250,
            height = 125,
        },
    }
end
local parsed = 0
wiki.loadPage = function(_, topic, _, mode)
    assert(topic == "Harbor notebook")
    if mode == 4 then
        parsed = parsed + 1
        return {parse = {text = {["*"] = markup}}}
    end
    return {query = {pages = {page()}}}
end
wiki.wiki_prettify = false

local intro = wiki:searchAndGetIntros("Harbor notebook", "en")[1]
assert(#intro.images == 1 and intro.images[1].title == "Harbor.png" and parsed == 0)
local full = wiki:getFullPage("Harbor notebook", "en")[1]
local illustrated = full
-- The stock source also lists the deliberately foreign fixture, which Reader filters.
assert(full.images[1].title == "Harbor.png" and full.images[1].caption == "Lead harbor illustration.")
assert(full.images[2].title == "Jetty.png" and full.images[2].caption == "A jetty & channel.")
assert(full.images[#full.images].title == "Signal.png" and parsed == 1)
more_setting.callback()
full = wiki:getFullPage("Harbor notebook", "en")[1]
assert(#full.images == 1 and parsed == 1)
lead_setting.callback()
assert(wiki:getFullPage("Harbor notebook", "en")[1].images == nil)
assert(wiki:searchAndGetIntros("Harbor notebook", "en")[1].images == nil)
print("Stock default settings, lead fallback, figure/gallery order and lead caption verified")

-- Exercise KOReader's actual TextBoxWidget line splitting and tap callback.
-- Only the screen, font measurements and optional async UI are mocked.
mock("ui/widget/container/inputcontainer", {extend = function(_, defaults) return defaults end})
mock("ui/font", {getFace = function() return {} end})
mock("dbg", {guard = function() end})
mock("ui/uimanager", {setDirty = function() end})
for _, name in ipairs{
    "ffi/blitbuffer", "ui/widget/container/framecontainer", "ui/geometry", "ui/gesturerange",
    "ui/widget/linewidget", "ui/rendertext", "ui/widget/container/rightcontainer", "ui/size",
    "ui/widget/textwidget", "ffi/utf8proc", "optmath",
} do
    mock(name, {})
end
local text_box = dofile(root .. "/frontend/ui/widget/textboxwidget.lua")
local characters = {}
for character in illustrated.extract:gmatch("[%z\1-\127\194-\244][\128-\191]*") do
    characters[#characters + 1] = character
end
assert(table.concat(characters) == extract)
local widget = setmetatable({
    images = illustrated.images,
    width = 340,
    height = 128,
    lines_per_page = 6,
    line_height_px = 20,
    use_xtext = false,
    charlist = characters,
    char_width = setmetatable({}, {__index = function() return 7 end}),
    dimen = {x = 0, y = 0},
    update = function() end,
}, {__index = text_box})
widget:_splitToLines()
assert(widget.scroll_force_to_page)
assert(#widget.vertical_string_list > #widget.images * widget.lines_per_page)
for line = 1, #widget.vertical_string_list do
    local page = (line - 1) / widget.lines_per_page + 1
    local image = page % 1 == 0 and widget.images[page] or nil
    assert(widget.line_num_to_image[line] == image)
    if image then
        assert(widget.vertical_string_list[line].width <= widget.width - image.width)
    end
end
assert(widget.line_num_to_image[1].title == "Harbor.png")
assert(widget.line_num_to_image[1 + widget.lines_per_page].title == "Jetty.png")
assert(widget.line_num_to_image[1 + 3 * widget.lines_per_page].title == "Signal.png")
widget.images[1].bb = {} -- Represents a raster already received by the widget.
assert(widget:onTapImage(nil, {pos = {x = widget.width - 1, y = 1}}))
assert(widget.image_show_alt_text == true)
assert(widget:onTapImage(nil, {pos = {x = widget.width - 1, y = 1}}))
assert(widget.image_show_alt_text == false)
widget.images = nil
widget:_splitToLines()
assert(widget.line_num_to_image == nil)
print("Stock text wraps around one ordered top-right image per screen page, with tap for caption")
