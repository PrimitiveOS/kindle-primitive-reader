-- Run using luajit from the pinned KOReader runtime directory.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")

local Device = require("device")
require("document/canvascontext"):init(Device)
local Registry = require("document/documentregistry")
local provider = Registry:getProviderFromKey("mupdf")
for _, filename in ipairs(arg) do
    local doc = assert(Registry:openDocument(filename, provider))
    local size = doc:getNativePageDimensions(1)
    local seed = { x0 = size.w * .05, y0 = size.h * .05, x1 = size.w * .95, y1 = size.h * .95 }
    doc.configurable.trim_page = 2
    doc.configurable.text_wrap = 0
    doc.bbox = { [1] = seed, odd = seed }
    local bounds = doc:getPageBBox(1)
    local name = assert(filename:match("([^/]+)%.pdf$"))
    print(string.format("BOUND %s %d %d %.6f %.6f %.6f %.6f",
        name, size.w, size.h, bounds.x0, bounds.y0, bounds.x1, bounds.y1))
    doc:close()
end
Device:exit()
