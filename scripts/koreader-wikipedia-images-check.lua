-- Run with the pinned macOS LuaJIT and the upstream KOReader source tree.
local root = assert(arg[1], "Pass the pinned KOReader source tree")
local fixture = assert(arg[2], "Pass the authored Wikimedia markup fixture")
local file = assert(io.open(fixture))
local markup = file:read("*a")
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
mock("util", {htmlToPlainText = function(value)
    return (value:gsub("<[^>]->", ""):gsub("&amp;", "&"))
end})
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
        extract = "Article text remains available.",
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
