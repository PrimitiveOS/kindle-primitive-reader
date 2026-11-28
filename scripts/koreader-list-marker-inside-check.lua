-- Measure inside and outside list markers in installed KOReader (CREngine).
-- Invoke from the KOReader installation: ./luajit this-script scratch font
-- The script writes both chapters to scratch, renders them with the same settings as Reader's
-- inside-marker test, and prints the start of each wrapped line and the marker ink for item one.
local scratch, font = arg[1], arg[2]
assert(scratch and font and not arg[3])
os.setlocale("C", "numeric")
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local Settings = require("luasettings")
G_reader_settings = Settings:wrap({})
require("document/canvascontext"):init(require("device"))
local Cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local lfs = require("libs/libkoreader-lfs")
assert(lfs.symlinkattributes(scratch, "mode") == "directory")
local cache = scratch .. "/cache"
if not lfs.attributes(cache) then assert(lfs.mkdir(cache)) end
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)

local item = "First item stays selectable and wraps onto a second line in this narrow reading column."
local function chapter(position)
    return ([[<html><body>
  <ol style="list-style-position:%s">
    <li>%s</li>
    <li>Second item remains a separate source block.</li>
  </ol>
</body></html>]]):format(position, item)
end

local function write(name, text)
    local path = scratch .. "/" .. name
    local file = assert(io.open(path, "wb"))
    file:write(text)
    file:close()
    return path
end

-- The page geometry matches the pinned list-marker replay (617 x 720, 30 px margins, 30 px font).
-- The comparison is qualitative (where lines start relative to each other), so the geometry of
-- Reader's own test (380 x 720) does not need to match.
local width, height, margin = 617, 720, 30
local function measure(label, path)
    local view = Cre.newDocView(width, height, 1)
    view:setIntProperty("crengine.render.block.rendering.flags", 0x03375131)
    view:setIntProperty("crengine.render.dpi", 96)
    view:setStyleSheet("./data/epub.css")
    view:setPageMargins(margin, margin, margin, margin)
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    view:setFontFace("Noto Sans")
    view:setFontSize(30)
    assert(view:loadDocument(path, false))
    view:renderDocument()
    local hits = assert(view:findAllText(item, false, false, 16, true, 0, 255))
    assert(#hits == 1, label .. " item not found once")
    local boxes = assert(view:getWordBoxesFromPositions(hits[1].start, hits[1]["end"], true))
    -- Group word boxes into lines by their top edge and keep each line's left-most word.
    local lines = {}
    for _, box in ipairs(boxes) do
        local line
        for _, existing in ipairs(lines) do
            if math.abs(existing.y0 - box.y0) < 4 then line = existing end
        end
        if not line then
            line = { y0 = box.y0, y1 = box.y1, x0 = box.x0 }
            lines[#lines + 1] = line
        end
        line.x0 = math.min(line.x0, box.x0)
        line.y1 = math.max(line.y1, box.y1)
    end
    table.sort(lines, function(a, b) return a.y0 < b.y0 end)
    print(label, "lines", #lines)
    for i, line in ipairs(lines) do
        print(label, "line", i, "x0", line.x0, "y0", line.y0, "y1", line.y1)
    end
    -- Marker ink on the first line: columns left of its first word, down to the margin edge.
    local pixels = BB.new(width, height, BB.TYPE_BB8)
    view:gotoPage(1)
    view:drawCurrentPage(pixels, false, false, false, false)
    local first = lines[1]
    local ink_min, ink_max = nil, nil
    for y = first.y0, first.y1 - 1 do
        for x = 0, first.x0 - 1 do
            if pixels:getPixel(x, y):getColor8().a < 192 then
                ink_min = ink_min and math.min(ink_min, x) or x
                ink_max = ink_max and math.max(ink_max, x) or x
            end
        end
    end
    print(label, "marker_ink_x", ink_min, ink_max)
    pixels:free()
    view:close()
    return lines, ink_min
end

local inside = write("inside.xhtml", chapter("inside"))
local outside = write("outside.xhtml", chapter("outside"))
local inside_lines, inside_ink = measure("inside", inside)
local outside_lines, outside_ink = measure("outside", outside)
-- Facts the Reader layout claims: the inside marker is part of the first line, so the
-- continuation lines start at the margin while the first line's text starts after the marker;
-- the outside marker hangs in the margin, so every line starts at the same x.
print("inside_first_text_x", inside_lines[1].x0, "inside_continuation_x", inside_lines[2] and inside_lines[2].x0)
print("outside_first_text_x", outside_lines[1].x0, "outside_continuation_x", outside_lines[2] and outside_lines[2].x0)
