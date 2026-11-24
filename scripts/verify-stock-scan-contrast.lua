require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
local Device = require("device")
local CanvasContext = require("document/canvascontext")
CanvasContext:init(Device)
CanvasContext:setColorRenderingEnabled(false)
local Registry = require("document/documentregistry")
local levels = {0.8, 1.0, 1.5, 2.0, 2.3, 4.0, 6.0, 10.0, 50.0}

local function authored_gray_ramp()
    local contents = {}
    for row = 0, 15 do
        contents[#contents + 1] = string.format("%.8f g 0 %d 80 20 re f\n", row / 15, 300 - row * 20)
    end
    local stream = table.concat(contents)
    local objects = {
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 80 320] /Contents 4 0 R >>",
        string.format("<< /Length %d >>\nstream\n%sendstream", #stream, stream),
    }
    local filename = DataStorage:getDataDir() .. "/authored-gray-ramp.pdf"
    local file = assert(io.open(filename, "wb"))
    file:write("%PDF-1.4\n")
    local offsets = {}
    for index, object in ipairs(objects) do
        offsets[index] = file:seek()
        file:write(index, " 0 obj\n", object, "\nendobj\n")
    end
    local xref = file:seek()
    file:write("xref\n0 5\n0000000000 65535 f \n")
    for _, offset in ipairs(offsets) do
        file:write(string.format("%010d 00000 n \n", offset))
    end
    file:write("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n", xref, "\n%%EOF\n")
    file:close()
    return filename
end

local function open(provider, filename)
    local doc = assert(Registry:openDocument(filename, Registry:getProviderFromKey(provider)))
    doc.configurable.text_wrap = 0
    doc.configurable.background_cleanup = 0
    return doc
end

local function render(doc, zoom, gamma)
    return assert(doc:renderPage(1, nil, zoom, 0, gamma, 1)).bb
end

local function sample(bb, x, y)
    return bb:getPixel(x, y):getColor8().a
end

local pdf = open("mupdf", authored_gray_ramp())
local neutral = render(pdf, 1, 1)
for _, gamma in ipairs(levels) do
    local frame = render(pdf, 1, gamma)
    local tones = {}
    for row = 0, 15 do
        local y = row * 20 + 10
        local original = sample(neutral, 20, y)
        local adjusted = sample(frame, 20, y)
        assert(adjusted == math.floor(255 * (original / 255) ^ gamma))
        tones[#tones + 1] = adjusted
    end
    print(string.format("PDF gamma %.1f | %s", gamma, table.concat(tones, " ")))
end
pdf:close()

-- The smaller render exposes gray edges in the authored DjVu's bitonal layer.
local djvu = open("djvu", assert(arg[1], "Pass tests/fixtures/beacon-atlas.djvu"))
local zoom = 0.37
local baseline = render(djvu, zoom, 1)
local locations = {}
local count = 0
for y = 0, baseline:getHeight() - 1, 3 do
    for x = 0, baseline:getWidth() - 1, 3 do
        local tone = sample(baseline, x, y)
        if locations[tone] == nil then
            locations[tone] = {x, y}
            count = count + 1
        end
    end
end
assert(count >= 64)
for _, gamma in ipairs(levels) do
    local frame = render(djvu, zoom, gamma)
    for tone, point in pairs(locations) do
        assert(sample(frame, point[1], point[2]) == tone)
    end
    print(string.format("DjVu gamma %.1f | %d sampled tones unchanged", gamma, count))
end
djvu:close()

-- Optional authored comic archives with the same sixteen-band grayscale ramp.
for index = 2, #arg do
    local comic = open("mupdf", arg[index])
    local original = render(comic, 1, 1)
    local x = math.floor(original:getWidth() / 2)
    local height = original:getHeight()
    local changed = 0
    for _, gamma in ipairs(levels) do
        local frame = render(comic, 1, gamma)
        for row = 0, 15 do
            local y = math.floor((row + 0.5) * height / 16)
            local tone = sample(original, x, y)
            local adjusted = sample(frame, x, y)
            assert(adjusted == math.floor(255 * (tone / 255) ^ gamma))
            if adjusted ~= tone then
                changed = changed + 1
            end
        end
    end
    assert(changed > 0)
    print(string.format("%s: %d gamma comparisons, %d changed tones", arg[index], #levels * 16, changed))
    comic:close()
end
Device:exit()
