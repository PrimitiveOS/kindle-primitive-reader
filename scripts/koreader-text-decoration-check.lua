-- Compare each selected word to the same publication with decorations forced off or on.
-- Pass an isolated parent directory, book path, and TSV of expected decoration and XPointers.
local scratch, book, list = assert(arg[1]), assert(arg[2]), assert(arg[3])
os.setlocale("C", "numeric")
require("setupkoenv")
local lfs = require("libs/libkoreader-lfs")
local cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local cache = scratch .. "/text-decoration-cache"
assert(lfs.symlinkattributes(scratch, "mode") == "directory" and lfs.mkdir(cache))
cre.initCache(cache, 0, true, 1)
cre.registerFont("/mnt/us/launcher/fonts/NotoSans-Regular.ttf")
local width, height = 1264, 1606
local cases = {}
local modes = {source = true, none = true, underline = true}
for row in io.lines(list) do
    local kind, start, stop = row:match("^([^\t]+)\t([^\t]+)\t([^\t]+)$")
    assert(kind == "none" or kind == "underline" or kind == "line-through" or kind == "overline")
    cases[#cases + 1] = {kind, start, stop}
    modes[kind] = true
end

local function gather(mode)
    local view = cre.newDocView(width, height, 1)
    local css = mode == "source" and "" or "* {text-decoration: " .. mode .. " !important;}"
    view:setStyleSheet("./data/epub.css", css)
    view:setFontSize(42)
    view:setFontFace("Noto Sans")
    assert(view:loadDocument(book, false))
    view:renderDocument()
    local frame = BB.new(width, height, BB.TYPE_BB8)
    local results = {}
    for i, c in ipairs(cases) do
        if view:isXPointerInDocument(c[2]) and view:isXPointerInDocument(c[3]) then
            view:gotoPage(view:getPageFromXPointer(c[2]))
            local boxes = view:getWordBoxesFromPositions(c[2], c[3], true) or {}
            if #boxes == 1 then
                local r = boxes[1]
                local x0, x1 = r.x0 + 2, r.x1 - 2
                local y0, y1 = r.y0 - 2, r.y1 + 2
                if x0 >= 0 and x1 < width and x1 > x0 and y0 >= 0 and y1 < height
                    and x1 - x0 <= 700 and y1 - y0 <= 160 then
                    view:drawCurrentPage(frame, false, false, false, false)
                    local pixels = {}
                    for y = y0, y1 do
                        for x = x0, x1 do
                            pixels[#pixels + 1] = string.char(frame:getPixel(x, y):getColor8().a)
                        end
                    end
                    results[i] = table.concat(pixels)
                end
            end
        end
    end
    frame:free()
    view:close()
    return results
end

local frames = {}
for mode in pairs(modes) do frames[mode] = gather(mode) end
local function difference(a, b)
    if not a or not b or #a ~= #b then return -1 end
    local count = 0
    for j = 1, #a do
        if math.abs(a:byte(j) - b:byte(j)) > 12 then count = count + 1 end
    end
    return count
end
for i, c in ipairs(cases) do
    local expected = c[1]
    local control = expected == "none" and "underline" or "none"
    print(string.format("ink %d %s %d %d", i, expected,
        difference(frames.source[i], frames[expected][i]),
        difference(frames.source[i], frames[control][i])))
end
print("completed")
