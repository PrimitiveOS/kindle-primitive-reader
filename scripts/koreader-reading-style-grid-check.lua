-- Run from the pinned native KOReader runtime with authored EPUB/XHTML specimens.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local Settings = require("luasettings")
G_reader_settings = Settings:wrap({})
require("document/canvascontext"):init(require("device"))
local Typeset = require("apps/reader/modules/readertypeset")
local StyleTweak = require("apps/reader/modules/readerstyletweak")
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local cjk, colors, cache, font, output = assert(arg[1]), assert(arg[2]), assert(arg[3]),
    assert(arg[4]), assert(arg[5])
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)

local applied = 0
local config = Settings:wrap({})
local controller = StyleTweak:new{ ui = {
    menu = { registerToMainMenu = function() end },
    doc_settings = config,
    handleEvent = function(_, event)
        assert(event.handler == "onApplyStyleSheet")
        applied = applied + 1
    end,
} }
controller:onReadSettings(config)
local menu = {}
controller:addToMainMenu(menu)
local function row(items, id)
    for _, item in ipairs(items) do
        if item.tweak_id == id then return item end
        if type(item.sub_item_table) == "table" then
            local found = row(item.sub_item_table, id)
            if found then return found end
        end
    end
end
local grid = assert(row(menu.style_tweaks.sub_item_table, "cjk_tailored"))
local monochrome = assert(row(menu.style_tweaks.sub_item_table, "pure_black_and_white"))
assert(grid.enabled_func() and monochrome.enabled_func())
assert(not grid.checked_func() and not monochrome.checked_func())
local function toggle(tweak, choice)
    local before = applied
    tweak.callback()
    assert(tweak.checked_func() == choice and applied == before + 1)
end

local width, height, margin = 617, 680, 30
local size = 40
local function open(book, face, scale, authored)
    local view = Cre.newDocView(width, height, 1)
    local typeset = setmetatable({ ui = {
        document = { setBlockRenderingFlags = function(_, flags)
            assert(flags == 0x03375131)
            view:setIntProperty("crengine.render.block.rendering.flags", flags)
        end },
        handleEvent = function(_, event) assert(event.handler == "onUpdatePos") end,
    }}, { __index = Typeset })
    typeset:setBlockRenderingMode(2)
    view:setStyleSheet("./data/epub.css", (authored or "") .. "\n" .. assert(controller:getCssText()))
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setPageMargins(margin, margin, margin, margin)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setIntProperty("crengine.style.cjk.width.scale.percent", scale)
    view:setFontFace(face)
    view:setFontSize(size)
    assert(view:loadDocument(book, false))
    view:renderDocument()
    return view
end

local function text(view, phrase, first)
    local hits = assert(view:findAllText(phrase, false, false, 16, true, 0, 255))
    assert(first or #hits == 1, phrase)
    local hit = hits[1]
    assert(view:getTextFromXPointers(hit.start, hit["end"]) == phrase)
    return hit
end

local function box(view, hit)
    view:gotoPage(view:getPageFromXPointer(hit.start))
    local boxes = assert(view:getWordBoxesFromPositions(hit.start, hit["end"], true))
    assert(#boxes >= 1)
    return boxes[1], boxes[#boxes]
end

local function frame(view, filename)
    local page = BB.new(width, height, BB.TYPE_BB8)
    view:drawCurrentPage(page, false, false, false, false)
    page:writePNG(output .. "/" .. filename .. ".png", false)
    return page
end

local authored = [[
#sample { text-indent: 1.2em !important; }
#t1 { text-align: justify !important; text-indent: 1.2em !important; }
]]
local anchor
local function grid_sample(scale, enabled)
    local view = open(cjk, "Harbour Test Gothic", scale, authored)
    local sample = text(view, "港波朝空海川山光")
    anchor = anchor or sample
    assert(sample.start == anchor.start and sample["end"] == anchor["end"])
    local base, last = box(view, sample)
    local latin = box(view, text(view, "HALF WIDE MIX"))
    local run = text(view, "朝の港で小さな船が潮風を待つ。", true)
    local stem, length = run["end"]:match("^(.-)%.(%d+)$")
    assert(stem and run.start == stem .. ".0")
    local first_y = view:getPosFromXPointer(run.start)
    local line = 0
    for at = 1, tonumber(length) do
        if view:getPosFromXPointer(stem .. "." .. at) > first_y then
            line = at
            break
        end
    end
    assert(line > 0)
    local start, stop = box(view, { start = run.start, ["end"] = stem .. "." .. line })
    local unit = size * scale / 100
    local raw_width = width - 2 * margin
    local usable = enabled and math.floor(raw_width / unit) * unit or raw_width
    local origin = margin + math.floor((raw_width - usable) / 2)
    -- The stylesheet parser rounds 1.2em to just under 48px. The grid rounds
    -- this indent to two cells when the available width is at least eight.
    local indent = enabled and 2 * unit or 47
    assert(latin.x0 == origin and base.x0 == origin + indent)
    assert(last.x1 - base.x0 == 8 * unit)
    assert(line == math.floor((usable - indent) / unit))
    assert(start.x0 == origin + indent and stop.x1 == origin + usable)
    local phase = enabled and "on" or "off"
    print(scale, phase, "origin", latin.x0, "indent", base.x0 - latin.x0, "sample", last.x1 - base.x0,
        "first line", line, start.x0, stop.x1, "grid width", usable)
    frame(view, ("stock-reading-cjk-grid-%d-%s"):format(scale, phase)):free()
    view:close()
end

for _, scale in ipairs({100, 125}) do
    grid_sample(scale, false)
    toggle(grid, true)
    grid_sample(scale, true)
    toggle(grid, false)
end

local color_geometry
local function color_sample(enabled)
    local view = open(colors, "Noto Sans", 100)
    local caption = box(view, text(view, "Slate"))
    local paper = box(view, text(view, "Tinted"))
    local geometry = table.concat({ caption.x0, caption.y0, caption.x1, caption.y1,
        paper.x0, paper.y0, paper.x1, paper.y1 }, ",")
    color_geometry = color_geometry or geometry
    assert(geometry == color_geometry)
    local phase = enabled and "on" or "off"
    local page = frame(view, "stock-reading-monochrome-" .. phase)
    local function gray(x, y) return page:getPixel(x, math.floor(y)):getColor8().a end
    local text_gray = 255
    for y = caption.y0, caption.y1 - 1 do
        for x = caption.x0, caption.x1 - 1 do
            text_gray = math.min(text_gray, gray(x, y))
        end
    end
    local border = gray(margin + 3, (caption.y0 + caption.y1) / 2)
    local fill = gray(caption.x1 + 8, (caption.y0 + caption.y1) / 2)
    local tint = gray(width - margin - 20, (paper.y0 + paper.y1) / 2)
    local background = gray(margin, height - margin)
    if enabled then
        assert(text_gray == 0 and border == 0 and fill == 255 and tint == 255 and background == 255)
    else
        assert(text_gray == 112 and border == 104 and fill == 187 and tint == 224 and background == 221)
    end
    print(phase, "ink", text_gray, "border", border, "fill", fill, "paper", tint, "body", background)
    page:free()
    view:close()
end
color_sample(false)
toggle(monochrome, true)
color_sample(true)
toggle(monochrome, false)
