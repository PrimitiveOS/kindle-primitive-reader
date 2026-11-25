-- Replay stock PDF viewport turns in the pinned native KOReader runtime.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
G_reader_settings = require("luasettings"):wrap({})
local Device = require("device")
require("document/canvascontext"):init(Device)
local Registry = require("document/documentregistry")
local View = require("apps/reader/modules/readerview")
local Paging = require("apps/reader/modules/readerpaging")
local UIManager = require("ui/uimanager")
local Geom = require("ui/geometry")
local BB = require("ffi/blitbuffer")
local Icon = require("ui/widget/iconwidget")

local source, output = assert(arg[1]), assert(arg[2])
local width, height = 1264, 1606
local pdf = assert(Registry:openDocument(source))
local native = pdf:getNativePageDimensions(1)
assert(pdf:getPageCount() == 2 and native.w == 600 and native.h == 800)
local zoom = width / native.w
local viewport = Geom:new{x = 0, y = 0, w = width, h = height}
local page_area = Geom:new{x = 0, y = 0, w = width, h = math.floor(native.h * zoom)}
local arrow_size = Device.screen:scaleBySize(16)
local ui = {
    document = pdf,
    zooming = {
        zoom_overlap_h = 40,
        zoom_overlap_v = 40,
        zoom_direction_vertical = false,
    },
}
local view = setmetatable({
    ui = ui,
    document = pdf,
    dimen = viewport:copy(),
    page_area = page_area,
    visible_area = viewport:copy(),
    dim_area = Geom:new{},
    page_scroll = false,
    page_overlap_enable = true,
    state = {offset = {x = 0, y = 0}, zoom = zoom, page = 1, rotation = 0, gamma = 1, saturation = 1},
    footer = {getHeight = function() return 0 end, onUpdateFooter = function() end},
    highlight = {},
    highlight_visible = false,
    dogear_visible = false,
    footer_visible = false,

    flipping = {paintTo = function() end},
    arrow = Icon:new{icon = "control.expand.alpha", width = arrow_size, height = arrow_size, alpha = true},
    view_modules = {},
    dialog = {},
    outer_page_color = BB.COLOR_WHITE,
}, {__index = View})
local paging = setmetatable({
    ui = ui,
    view = view,
    current_page = 1,
    number_of_pages = pdf:getPageCount(),
    zoom_mode = "pagewidth",
    visible_area = view.visible_area:copy(),
    page_area = page_area,
    setPagePosition = function() end,
    _gotoPage = function(self, page)
        self.current_page = page
        view.state.page = page
        view.visible_area = viewport:copy()
        self.visible_area = view.visible_area:copy()
    end,
}, {__index = Paging})
ui.paging, ui.view = paging, view
ui.handleEvent = function(_, event)
    if event.handler == "onViewRecalculate" then
        paging:onViewRecalculate(unpack(event.args))
    end
end
UIManager.setDirty = function() end
UIManager.nextTick = function() end

assert(view:isOverlapAllowed())
paging.zoom_mode = "pageheight"
assert(not view:isOverlapAllowed())
paging.zoom_mode = "page"
assert(not view:isOverlapAllowed())
paging.zoom_mode = "pagewidth"
view.page_scroll = true
assert(not view:isOverlapAllowed())
view.page_scroll = false

local function paint(style)
    view:onSetOverlapStyle(style, true)
    local frame = BB.new(width, height, BB.TYPE_BB8)
    view:paintTo(frame, 0, 0)
    return frame
end

-- The real paging handler and ReaderView clamp to the page end before a page turn.
assert(paging:onGotoViewRel(1, {}))
local displacement = view.visible_area.y
assert(displacement == page_area.h - height and displacement > 0)
assert(paging.current_page == 1 and view.dim_area.y == 0)
assert(view.dim_area.h == height - displacement and view.dim_area.w == width)

local plain = paint("none")
local dashed = paint("dashed_line")
local boundary = view.dim_area.h
local changes = 0
for y = 0, height - 1 do
    for x = 0, width - 1 do
        local a = plain:getPixel(x, y):getColor8().a
        local b = dashed:getPixel(x, y):getColor8().a
        if a ~= b then
            assert(y >= boundary and y < boundary + 3 and b == 136)
            changes = changes + 1
        end
    end
end
assert(changes == math.floor(width / 20) * 14 * 3)
dashed:writePNG(output, false)
dashed:free()
local line = paint("line")
for x = 0, width - 1 do
    for y = boundary, boundary + 2 do
        assert(line:getPixel(x, y):getColor8().a == 136)
    end
    assert(line:getPixel(x, boundary - 1):getColor8().a == plain:getPixel(x, boundary - 1):getColor8().a)
    assert(line:getPixel(x, boundary + 3):getColor8().a == plain:getPixel(x, boundary + 3):getColor8().a)
end
line:free()
local arrow = paint("arrow")
local arrow_pixels = 0
for y = boundary - arrow_size, boundary + arrow_size do
    for x = 0, arrow_size * 2 do
        if arrow:getPixel(x, y):getColor8().a ~= plain:getPixel(x, y):getColor8().a then
            assert(x < arrow_size and math.abs(y - boundary) <= math.ceil(arrow_size / 2))
            arrow_pixels = arrow_pixels + 1
        end
    end
end
assert(arrow_pixels > 0)
arrow:free()
local dimmed = paint("dim")
local dim_changes = 0
for y = 0, height - 1 do
    for x = 0, width - 1, 4 do
        local a = plain:getPixel(x, y):getColor8().a
        local b = dimmed:getPixel(x, y):getColor8().a
        if a ~= b then
            assert(y < boundary and b > a)
            dim_changes = dim_changes + 1
        end
    end
end
assert(dim_changes > 0)
dimmed:free()
plain:free()

assert(paging:onGotoViewRel(-1, {}))
assert(view.visible_area.y == 0 and view.dim_area.y == displacement)
assert(view.dim_area.h == boundary and view.dim_area.w == width)
assert(paging:onGotoViewRel(1, {}))
assert(paging:onGotoViewRel(1, {}))
assert(paging.current_page == 2 and view.dim_area:isEmpty())

-- A horizontal page step may retain only the width of the old viewport.
view.visible_area = viewport:copy()
paging.visible_area = view.visible_area:copy()
page_area.w = width + 300
view.state.zoom = page_area.w / native.w
assert(paging:onGotoViewRel(1, {}))
assert(view.visible_area.x == 300 and view.dim_area.w == width - 300)
assert(view.dim_area.y == 0 and view.dim_area.h == height and paging.current_page == 2)
pdf:close()
print(("Stock fixed-page overlap passed: %d px clamped vertical pan, then %d px shared height"):format(displacement, boundary))

if arg[3] then
    local epub, font, cache = arg[3], assert(arg[4]), assert(arg[5])
    local Cre = require("libs/libkoreader-cre")
    local CreDocument = require("document/credocument")
    local Rolling = require("apps/reader/modules/readerrolling")
    Cre.initCache(cache, 0, true, 1)
    Cre.registerFont(font)
    local doc_view = Cre.newDocView(width, height, CreDocument.SCROLL_VIEW_MODE)
    doc_view:setStyleSheet("./data/epub.css", nil)
    doc_view:setVisiblePageCount(1, false)
    doc_view:setHeaderInfo(0)
    doc_view:setFontFace("Noto Sans")
    doc_view:setFontSize(42)
    assert(doc_view:loadDocument(epub, false))
    doc_view:renderDocument()
    local document = setmetatable({
        _document = doc_view,
        info = {doc_height = doc_view:getFullHeight()},
    }, {__index = CreDocument})
    local scroll_view = {
        view_mode = "scroll",
        footer_visible = true,
        page_overlap_enable = true,
        dim_area = Geom:new{},
        footer = {settings = {}, getHeight = function() return 74 end},
    }
    local scroll_ui = {
        dimen = Geom:new{w = width, h = 1680},
        document = document,
        keyselection = {isActive = function() return false end},
    }
    local rolling = setmetatable({
        ui = scroll_ui,
        view = scroll_view,
        current_pos = 0,
        configurable = {font_size = 24, line_spacing = 115},
    }, {__index = Rolling})
    scroll_ui.handleEvent = function(_, event)
        if event.handler == "onPosUpdate" then rolling.current_pos = event.args[1] end
    end
    local pitch = Device.screen:scaleBySize(rolling.configurable.font_size * 1.1 * 1.15)

    G_reader_settings:saveSetting("copt_overlap_lines", 2)
    rolling:onGotoViewRel(1)
    assert(rolling.current_pos == height - pitch * 2)
    assert(document:getCurrentPos() == math.floor(rolling.current_pos))
    assert(scroll_view.dim_area.y == 0 and scroll_view.dim_area.h == pitch * 2)

    local old = rolling.current_pos
    rolling:onGotoViewRel(0.5)
    assert(rolling.current_pos - old == 0.5 * height - pitch * 2)
    assert(scroll_view.dim_area:isEmpty())

    G_reader_settings:saveSetting("copt_overlap_lines", 10)
    old = rolling.current_pos
    rolling:onGotoViewRel(-1)
    assert(old - rolling.current_pos == height - pitch * 10)
    assert(scroll_view.dim_area.y == height - pitch * 10)
    assert(scroll_view.dim_area.h == pitch * 10)

    local max_pos = document.info.doc_height - height
    rolling:_gotoPos(max_pos - 20, false)
    rolling:onGotoViewRel(1)
    assert(rolling.current_pos == max_pos and scroll_view.dim_area:isEmpty())
    print(("Stock EPUB overlap passed: %dpx line pitch, 2/10 lines, fractional pan, and end clamping"):format(pitch))
end
