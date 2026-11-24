-- Replay the actual Embedded Fonts control against pinned KOReader and an authored EPUB.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
local Device = require("device")
require("document/canvascontext"):init(Device)
local Notification = require("ui/widget/notification")
local Typeset = require("apps/reader/modules/readertypeset")
local CreDocument = require("document/credocument")
local CreOptions = require("ui/data/creoptions")
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local sha = require("ffi/sha2").md5
local source, no_fonts, cache, noto, silk, output =
    assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4]), assert(arg[5]), assert(arg[6])
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(noto)
Cre.registerFont(silk)

local option
for _, panel in ipairs(CreOptions) do
    for _, entry in ipairs(panel.options) do
        if entry.name == "embedded_fonts" then option = entry end
    end
end
assert(option and option.default_value == 1 and option.event == "ToggleEmbeddedFonts")

local width, height = 1264, 1606
local function open(path)
    local view = Cre.newDocView(width, height, 1)
    view:setStyleSheet("./data/epub.css", "")
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(54, 54, 54, 54)
    view:setFontFace("Noto Sans")
    view:setFontSize(42)
    view:setIntProperty("crengine.render.dpi", 96)
    assert(view:loadDocument(path, false))
    view:renderDocument()
    return setmetatable({ _document = view }, { __index = CreDocument })
end

local document = open(source)
local view = document._document
local configurable = { embedded_css = 1, embedded_fonts = 1 }
local updates, notices = 0, 0
Notification.notify = function() notices = notices + 1 end
local controller = setmetatable({
    configurable = configurable,
    ui = {
        document = document,
        handleEvent = function(_, event)
            assert(event.handler == "onUpdatePos")
            updates = updates + 1
            view:renderDocument()
        end,
    },
}, {__index = Typeset})
assert(option.enabled_func(configurable, document))
local blank = open(no_fonts)
assert(not option.enabled_func(configurable, blank))
blank._document:close()

local function locate(label)
    local found = assert((view:findAllText(label, false, false, 8, true, 0, 255) or {})[1], label)
    assert(view:getTextFromXPointers(found.start, found["end"]) == label, label)
    return found
end
local samples = {
    embedded = locate("Harbor lettering"),
    installed = locate("Signal letters"),
    ordinary = locate("Ordinary type"),
    style = locate("Printed inset"),
}
local function measure(anchor)
    local boxes = view:getWordBoxesFromPositions(anchor.start, anchor["end"], true)
    assert(#boxes >= 1)
    return boxes[1].x0, boxes[#boxes].x1 - boxes[1].x0
end
local function inspect(suffix)
    local page = view:getPageFromXPointer(samples.embedded.start)
    view:gotoPage(page)
    local frame = BB.new(width, height, BB.TYPE_BB8)
    view:drawCurrentPage(frame, false, false, false, false)
    if suffix then frame:writePNG(output .. "/embedded-fonts-stock-" .. suffix .. ".png") end
    local measurements = {}
    for name, range in pairs(samples) do
        measurements[name] = { measure(range) }
    end
    local digest = sha(BB.tostring(frame))
    frame:free()
    return measurements, digest
end

local before, first = inspect("on")
assert(controller:onToggleEmbeddedFonts(false))
assert(configurable.embedded_fonts == 0 and option.enabled_func(configurable, document))
local off, second = inspect("off")
assert(first ~= second and before.embedded[2] ~= off.embedded[2])
for _, sample in ipairs({"installed", "ordinary", "style"}) do
    for axis = 1, 2 do
        assert(before[sample][axis] == off[sample][axis], sample)
    end
end
assert(controller:onToggleEmbeddedStyleSheet(false))
assert(not option.enabled_func(configurable, document))
local no_css = inspect()
assert(off.style[2] ~= no_css.style[2])
assert(controller:onToggleEmbeddedStyleSheet(true))
assert(configurable.embedded_fonts == 0 and option.enabled_func(configurable, document))
assert(controller:onToggleEmbeddedFonts(true))
local after, restored = inspect()
assert(first == restored and before.embedded[2] == after.embedded[2])
assert(updates == 4 and notices == 4)
view:close()
print(string.format(
    "Embedded fonts control: remote font available, blank and styles-off disabled; bundled %d -> %d, installed %d unchanged, source and pixels restored",
    before.embedded[2], off.embedded[2], off.installed[2]
))
