-- Run from KOReader with luajit, an isolated cache directory, a book, and a TSV.
-- Each row contains sup/sub and start/end XPointers for a body glyph and a script glyph.
-- Only offsets in pixels and correlation scores are emitted. No book pixels are saved.
local scratch, book, list = assert(arg[1]), assert(arg[2]), assert(arg[3])
assert(not arg[4])
os.setlocale("C", "numeric")
require("setupkoenv")
local lfs = require("libs/libkoreader-lfs")
local cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local cache = scratch .. "/baseline-ink-cache"
assert(lfs.symlinkattributes(scratch, "mode") == "directory")
assert(lfs.mkdir(cache))
cre.initCache(cache, 0, true, 1)
cre.registerFont("/mnt/us/launcher/fonts/NotoSans-Regular.ttf")
local width, height = 1264, 1606
local cases = {}
for row in io.lines(list) do
    local fields = {}
    for field in row:gmatch("[^\t]+") do fields[#fields + 1] = field end
    assert(#fields == 5 and (fields[1] == "sup" or fields[1] == "sub"))
    cases[#cases + 1] = fields
end

local function box(view, start, stop)
    local rects = view:getWordBoxesFromPositions(start, stop, true) or {}
    if #rects ~= 1 then return end
    local r = rects[1]
    if r.x0 >= 0 and r.x1 <= width and r.x1 - r.x0 >= 3 and r.x1 - r.x0 <= 120 then
        return r
    end
end

-- Use the same vertical origin for both strips. The selection box's vertical
-- edge can cut off a descender, so obtain the ink from the surrounding rows.
local function strip(frame, rect, origin)
    if origin < 53 or origin + 53 >= height then return end
    local profile = {}
    for y = -53, 53 do
        local count = 0
        for x = rect.x0 + 1, rect.x1 - 2 do
            if frame:getPixel(x, origin + y):getColor8().a < 140 then count = count + 1 end
        end
        profile[y] = count
    end
    return profile
end

local function sample(css)
    local view = cre.newDocView(width, height, 1)
    view:setStyleSheet("./data/epub.css", css)
    view:setFontFace("Noto Sans")
    view:setFontSize(42)
    assert(view:loadDocument(book, false))
    view:renderDocument()
    local frame = BB.new(width, height, BB.TYPE_BB8)
    local samples = {}
    for i, c in ipairs(cases) do
        if view:isXPointerInDocument(c[2]) and view:isXPointerInDocument(c[4]) then
            view:gotoPage(view:getPageFromXPointer(c[4]))
            view:drawCurrentPage(frame, false, false, false, false)
            local body = box(view, c[2], c[3])
            local script = box(view, c[4], c[5])
            if body and script and math.abs(body.x0 - script.x0) < 110 and math.abs(body.y0 - script.y0) < 50 then
                local origin = math.floor((body.y0 + body.y1) / 2)
                samples[i] = {strip(frame, body, origin), strip(frame, script, origin)}
            end
        end
    end
    frame:free()
    view:close()
    return samples
end

-- Align the rasterised strokes at each candidate displacement. The body glyph
-- controls for movement of the complete line when the script changes its height.
local function displacement(control, original)
    if not control or not original then return end
    local best, value
    for dy = -25, 25 do
        local overlap, union = 0, 0
        for y = -28, 28 do
            local a, b = control[y], original[y + dy]
            overlap = overlap + math.min(a, b)
            union = union + math.max(a, b)
        end
        if union >= 12 then
            local score = overlap / union
            if not best or score > best then best, value = score, dy end
        end
    end
    return value, best
end

local source = sample("")
local baseline = sample("sup, sub { vertical-align: baseline !important; }")
for i, c in ipairs(cases) do
    local a, b = source[i], baseline[i]
    if a and b then
        local body, body_score = displacement(b[1], a[1])
        local script, script_score = displacement(b[2], a[2])
        if body and script then
            print(string.format("baseline %d %s %d %.3f %.3f", i, c[1],
                body - script, body_score, script_score))
        end
    end
end
print("completed")
