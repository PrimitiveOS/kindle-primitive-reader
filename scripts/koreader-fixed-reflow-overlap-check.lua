-- Run from the pinned native KOReader runtime with a transient KO_HOME.
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
UIManager.setDirty = function() end
UIManager.nextTick = function() end

local width, height = 1264, 1606
local function replay(source, output)
    local pdf = assert(Registry:openDocument(source))
    assert(pdf:getPageCount() == 2)
    local settings = pdf.configurable
    settings.text_wrap = 1
    settings.trim_page = 3
    settings.max_columns = 1
    settings.auto_straighten = 0
    settings.background_cleanup = 0
    settings.font_size = 1.3
    pdf.bbox = { [1] = {x0 = 0, y0 = 0, x1 = 600, y1 = 800},
                 [2] = {x0 = 0, y0 = 0, x1 = 600, y1 = 800} }
    local function area(page)
        local dimensions = pdf:getPageDimensions(page, 1, 0)
        return Geom:new{x = 0, y = 0, w = dimensions.w, h = dimensions.h}
    end
    local page_area = area(2)
    assert(page_area.h > height, ("Short reflowed fixture (%d px)"):format(page_area.h))
    local function viewport(page_geometry)
        return Geom:new{x = 0, y = 0, w = math.min(page_geometry.w, width), h = math.min(page_geometry.h, height)}
    end
    local ui = {
        document = pdf,
        zooming = {zoom_overlap_h = 40, zoom_overlap_v = 40, zoom_direction_vertical = false},
    }
    local view = setmetatable({
        ui = ui,
        document = pdf,
        dimen = Geom:new{x = 0, y = 0, w = width, h = height},
        page_area = page_area,
        visible_area = viewport(page_area),
        dim_area = Geom:new{},
        page_scroll = false,
        page_overlap_enable = true,
        page_overlap_style = "dashed_line",
        state = {offset = {x = 0, y = 0}, zoom = 1, page = 2, rotation = 0, gamma = 1, saturation = 1},
        footer = {getHeight = function() return 0 end, onUpdateFooter = function() end},
        highlight = {},
        highlight_visible = false,
        dogear_visible = false,
        footer_visible = false,
        flipping = {paintTo = function() end},
        view_modules = {},
        dialog = {},
        outer_page_color = BB.COLOR_WHITE,
    }, {__index = View})
    local paging = setmetatable({
        ui = ui,
        view = view,
        current_page = 2,
        number_of_pages = pdf:getPageCount(),
        zoom_mode = "page",
        visible_area = view.visible_area:copy(),
        page_area = page_area,
        setPagePosition = function() end,
        _gotoPage = function(self, page)
            self.current_page = page
            view.state.page = page
            view.page_area = area(page)
            view.visible_area = viewport(view.page_area)
            self:onViewRecalculate(view.visible_area, view.page_area)
        end,
    }, {__index = Paging})
    ui.paging, ui.view = paging, view
    ui.handleEvent = function(_, event)
        if event.handler == "onViewRecalculate" then
            paging:onViewRecalculate(unpack(event.args))
        end
    end
    local function reset()
        view.visible_area.y = 0
        paging:onViewRecalculate(view.visible_area, view.page_area)
        view.dim_area:clear()
    end
    local function paint()
        local frame = BB.new(width, height, BB.TYPE_BB8)
        frame:fill(BB.COLOR_WHITE)
        view:paintTo(frame, 0, 0)
        return frame
    end

    assert(view:isOverlapAllowed())
    settings.text_wrap = 0
    assert(not view:isOverlapAllowed())
    settings.text_wrap = 1
    local limit = page_area.h - height
    local move = math.min(limit, math.ceil(height * 0.6))
    assert(paging:onGotoViewRel(1, {}))
    assert(view.visible_area.y == move and paging.current_page == 2)
    assert(view.dim_area.y == 0 and view.dim_area.h == height - move)
    local marked = paint()
    marked:writePNG(output, false)

    reset()
    view.page_overlap_enable = false
    assert(paging:onGotoViewRel(1, {}))
    assert(view.visible_area.y == move and view.dim_area:isEmpty())
    local plain = paint()
    local changed = 0
    for y = 0, height - 1 do
        for x = 0, width - 1 do
            if plain:getPixel(x, y):getColor8().a ~= marked:getPixel(x, y):getColor8().a then
                assert(y >= height - move and y < height - move + 3)
                changed = changed + 1
            end
        end
    end
    assert(changed > width)
    marked:free()
    plain:free()

    view.page_overlap_enable = true
    assert(paging:onGotoViewRel(-1, {}))
    assert(view.visible_area.y == 0 and view.dim_area.y == move)
    assert(view.dim_area.h == height - move)
    assert(paging:onGotoViewRel(-1, {}))
    assert(paging.current_page == 1 and view.dim_area:isEmpty())

    print(("Stock reflow overlap passed: canvas %dx%d, pan %d, shared %d"):format(
        page_area.w, page_area.h, move, height - move))
    pdf:close()
end

replay(assert(arg[1]), assert(arg[2]))
if arg[3] then replay(arg[3], assert(arg[4])) end
