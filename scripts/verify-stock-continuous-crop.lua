require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
local Device = require("device")
require("document/canvascontext"):init(Device)
local Registry = require("document/documentregistry")
local doc = assert(Registry:openDocument(assert(arg[1]), Registry:getProviderFromKey("mupdf")))
local Geom = require("ui/geometry")
local ReaderView = require("apps/reader/modules/readerview")
local ReaderPaging = require("apps/reader/modules/readerpaging")
local ReaderZooming = require("apps/reader/modules/readerzooming")
local odd = { x0 = 60, y0 = 70, x1 = 550, y1 = 740 }
local even = { x0 = 80, y0 = 80, x1 = 530, y1 = 745 }
doc.bbox = { [1] = odd, [2] = even, odd = odd, even = even }
doc.configurable.text_wrap = 0
doc.configurable.auto_straighten = 0

local view = setmetatable({
    ui = { paging = true }, document = doc, page_scroll = true, footer_visible = false,
    visible_area = Geom:new{ x = 0, y = 0, w = 1264, h = 1606 }, page_gap = { height = 24 },
    state = { offset = Geom:new{ x = 0, y = 0 }, rotation = 0, gamma = 1, saturation = 1 },
}, { __index = ReaderView })
function view:onBBoxUpdate(bbox) self.use_bbox = bbox ~= nil end
local zoom = setmetatable({ ui = { document = doc, view = view }, view = view,
    dimen = Geom:new{ w = 1264, h = 1606 }, rotation = 0, zoom_mode = "contentwidth" },
    { __index = ReaderZooming })
local paging = setmetatable({ ui = { document = doc }, view = view, current_page = 1 }, { __index = ReaderPaging })
function paging:getPagePosition(page) return page == 1 and 0.8 or 0 end
function paging:_gotoPage(page)
    self.current_page = page
    self.view.state.page = page
    self.view.state.zoom = zoom:getZoom(page)
end
local function verify_scroll(mode, direction)
    assert(#view.page_states == 2)
    for _, state in ipairs(view.page_states) do
        local area = state.page_area
        local x = state.visible_area.x / state.zoom + 12
        local y = state.visible_area.y / state.zoom + 12
        local frame = assert(view:pageToScreenTransform(state.page, Geom:new{ x = x, y = y, w = 1, h = 1 }))
        local hit = assert(view:screenToPageTransform(frame))
        assert(hit.page == state.page and math.abs(hit.x - x) < 0.5 and math.abs(hit.y - y) < 0.5)
        print(string.format("ROW mode=%d direction=%s page=%d zoom=%.6f area=(%.3f,%.3f,%.3f,%.3f) visible_h=%d hit=(%.3f,%.3f)",
            mode, direction, state.page, state.zoom, area.x / state.zoom, area.y / state.zoom,
            area.w / state.zoom, area.h / state.zoom, state.visible_area.h, hit.x, hit.y))
    end
end
for _, mode in ipairs{ 0, 1, 2, 0, 1 } do
    doc.configurable.trim_page = mode
    paging:_gotoPage(1)
    paging:onInitScrollPageStates()
    verify_scroll(mode, "forward")
    paging:_gotoPage(2)
    paging:onInitScrollPageStates()
    view.page_states = paging:genPageStatesFromBottom(view.page_states[1],
        Geom:new{ w = 1264, h = 1606 }, Geom:new{ y = -500 })
    verify_scroll(mode, "backward")
end
local function box(mode, page)
    doc.configurable.trim_page = mode
    local b = doc:getPageBBox(page)
    print(string.format("BOX mode=%d page=%d (%.3f,%.3f,%.3f,%.3f)", mode, page, b.x0, b.y0, b.x1, b.y1))
    return b
end
assert(box(0, 1) == odd)
assert(box(0, 2) == even)
assert(box(0, 3) == odd)
assert(box(0, 4) == even)
assert(box(1, 1).x0 < odd.x0)
assert(box(1, 2).x1 > even.x1)
assert(box(1, 4) == even)
assert(box(2, 1).x0 > odd.x0)
doc.configurable.text_wrap = 1
assert(box(1, 1) == odd)
assert(box(2, 2) == even)
doc.configurable.text_wrap = 0
local revised_odd = { x0 = 120, y0 = 92, x1 = 500, y1 = 700 }
doc.bbox[3] = revised_odd
doc.bbox.odd = revised_odd
assert(box(0, 1) == odd)
assert(box(0, 3) == revised_odd)
doc.bbox.even = nil
doc.bbox[2] = nil
assert(box(0, 4) == revised_odd)
doc.bbox = {}
local used = box(0, 2)
assert(used.x0 == 120 and used.x1 == 590)
-- The viewport widens only when KOPT adds padding to detected bounds.
doc.bbox = { [1] = odd, [2] = even, odd = odd, even = even }
local native = {
    [1] = { {14, 108, 409, 649}, {120, 120, 589, 685} },
    [2] = { {170, 150, 409, 649}, {120, 120, 439, 539} },
}
local function assert_near(actual, expected)
    assert(math.abs(actual - expected) < 0.002,
        string.format("%.5f != %.5f", actual, expected))
end

for _, mode in ipairs{ 3, 0, 1, 2 } do
    doc.configurable.trim_page = mode
    for _, margin in ipairs{ 0, 0.05, 0.10, 0.25, 0.40, 0.55, 0.70, 0.85, 1.00, 1.50, 0.10 } do
        doc.configurable.page_margin = margin
        for page = 1, 2 do
            paging:_gotoPage(page)
            local width = 600
            if mode == 0 then
                local b = doc.bbox[page]
                width = b.x1 - b.x0
            elseif mode ~= 3 then
                local x0, y0, x1, y1 = unpack(native[mode][page])
                local screen_padding = margin * 300
                local pad = (x1 - x0) * screen_padding / (1264 - screen_padding) / 2
                x0, y0, x1, y1 = x0 - pad, y0 - pad, x1 + pad, y1 + pad
                if mode == 1 then
                    x0, y0, x1, y1 = math.max(0, x0), math.max(0, y0), math.min(599, x1), math.min(799, y1)
                end
                local b = doc:getPageBBox(page)
                assert_near(b.x0, x0)
                assert_near(b.y0, y0)
                assert_near(b.x1, x1)
                assert_near(b.y1, y1)
                width = x1 - x0
            end
            assert_near(view.state.zoom, 1264 / width)
        end
        print(string.format("MARGIN mode=%d inches=%.2f | both pages matched", mode, margin))
    end
end

-- Seeded bounds may extend beyond their editable seed. An oversized box
-- is rejected by fixed-page zoom and falls back to the native page.
doc.configurable.trim_page = 2
doc.configurable.page_margin = 1.50
doc.bbox[1] = { x0 = 165, y0 = 145, x1 = 412, y1 = 655 }
paging:_gotoPage(1)
local tight = doc:getPageBBox(1)
assert_near(tight.x0, 103.93735)
assert_near(tight.x1, 475.06265)
doc.bbox[2] = { x0 = 0, y0 = 0, x1 = 600, y1 = 800 }
paging:_gotoPage(2)
assert(not view.use_bbox)
assert_near(view.state.zoom, 1264 / 600)
print("MARGIN tight seed can expand, oversized seed fit uses page dimensions")
doc:close()
Device:exit()
