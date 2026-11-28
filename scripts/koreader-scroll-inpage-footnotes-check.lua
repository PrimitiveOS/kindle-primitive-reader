-- Run from the pinned KOReader build with its Lua source on LUA_PATH.
-- Records whether the stock In-page EPUB footnotes tweak draws note bands in
-- scroll view, as KOReader's CRE page splitter would. Writes PNG frames for
-- visual comparison. Arguments: scratch directory, KOReader runtime root, book, font, output directory.
local scratch, roots, book, font, out = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4]), assert(arg[5])
os.setlocale("C", "numeric")
require("setupkoenv")
local cre = require("libs/libkoreader-cre")
local lfs = require("libs/libkoreader-lfs")
local cache = scratch .. "/scroll-inpage-cache"
assert(lfs.symlinkattributes(scratch, "mode") == "directory" and lfs.mkdir(cache))
assert(lfs.symlinkattributes(out, "mode") == "directory")
cre.initCache(cache, 0, true, 1)
cre.registerFont(font)

-- The stock tweak's own CSS, read from the pinned runtime rather than retyped.
local tweaks = assert(io.open(roots .. "/frontend/ui/data/css_tweaks.lua")):read("*a")
local tweak_css = assert(tweaks:match('id = "footnote%-inpage_epub".-css = %[%[(.-)%]%]'))

local width, height = 1264, 1680
local doc = cre.newDocView(width, height, 1)
doc:setStyleSheet("./data/epub.css", tweak_css)
doc:setFontSize(42)
doc:setFontFace("Noto Sans")
assert(doc:loadDocument(book, false))
doc:setViewDimen(width, height)

local buffer = require("ffi/blitbuffer").new(width, height)
local function capture(name)
    doc:drawCurrentPage(buffer, false, false, false, false)
    buffer:writePNG(out .. "/" .. name .. ".png", false)
end

-- Page view: first screens of the book.
doc:setViewMode(1)
doc:renderDocument()
for page = 0, 2 do
    doc:gotoPage(page, true)
    capture(string.format("page-%02d", page))
end

-- Scroll view: consecutive screens of the same book.
doc:setViewMode(0)
doc:renderDocument()
local full = assert(doc:getFullHeight(), "the raw document must report its full height")
print("scroll full height", full)
local steps = math.min(6, math.floor(full / height) + 1)
for i = 0, steps - 1 do
    doc:gotoPos(i * height)
    capture(string.format("scroll-%02d", i))
end
doc:close()
print("ok")
