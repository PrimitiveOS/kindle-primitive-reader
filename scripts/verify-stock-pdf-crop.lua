-- Run using luajit from the pinned KOReader runtime directory.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")

local Device = require("device")
require("document/canvascontext"):init(Device)
local Registry = require("document/documentregistry")
local doc = assert(Registry:openDocument(assert(arg[1]), Registry:getProviderFromKey("mupdf")))
local Math = require("optmath")
local odd = { x0 = 60, y0 = 70, x1 = 550, y1 = 740 }
local even = { x0 = 80, y0 = 80, x1 = 530, y1 = 745 }
doc.configurable.trim_page = 2
doc.configurable.text_wrap = 0
doc.bbox = { [1] = odd, [2] = even, [Math.oddEven(1)] = odd, [Math.oddEven(2)] = even }

for page = 1, 4 do
    local bounds = doc:getPageBBox(page)
    print(string.format("BOUND %d %.6f %.6f %.6f %.6f margin=%.4f",
        page, bounds.x0, bounds.y0, bounds.x1, bounds.y1, doc.configurable.page_margin))
end

doc:close()
Device:exit()
