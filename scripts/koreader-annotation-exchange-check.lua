-- Run in the pinned Mac KOReader runtime with a scratch directory and the authored Reader EPUB.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local Settings = require("luasettings")
G_reader_settings = Settings:wrap({})
local Device = require("device")
require("document/canvascontext"):init(Device)
local Annotation = require("apps/reader/modules/readerannotation")
local Cre = require("libs/libkoreader-cre")
local CREDocument = require("document/credocument")
local source, font, scratch = assert(arg[1]), assert(arg[2]), assert(arg[3])
Cre.initCache(scratch .. "/cache", 0, true, 1)
Cre.registerFont(font)

local baseline = {
    { page = "/body/DocFragment[1]/body/p[1]", datetime = "2020-01-01 00:00:00" },
    { page = "/body/DocFragment[1]/body/p[2]/text().0",
      pos0 = "/body/DocFragment[1]/body/p[2]/text().0",
      pos1 = "/body/DocFragment[1]/body/p[2]/text().13",
      datetime = "2020-01-02 00:00:00", drawer = "lighten", note = "An older note" },
    { page = "/body/DocFragment[1]/body/p[3]", datetime = "2035-01-01 00:00:00" },
}
local incoming = {
    { page = baseline[2].page, pos0 = baseline[2].pos0, pos1 = baseline[2].pos1,
      datetime = "2020-01-02 00:00:00", datetime_updated = "2020-01-03 00:00:00",
      drawer = "underscore", note = "The updated note" },
}
local function reader(file, rolling, rows)
    return setmetatable({
        document = { file = file },
        ui = { rolling = rolling, paging = not rolling and true or nil },
        annotations = rows,
    }, { __index = Annotation })
end
local stock = reader(source, true, incoming)
local stock_folder = stock:getExportAnnotationsFilepath()
assert(stock_folder == source:gsub("%.[^./]+$", ".sdr") .. "/" ..
    assert(source:match("([^/]+)$")) .. ".annotations.lua")
G_reader_settings:saveSetting("annotations_export_folder", scratch)
G_reader_settings:saveSetting("device_id", "stock-reference-sender")
local dest = stock:getExportAnnotationsFilepath()
assert(dest == scratch .. "/" .. assert(source:match("([^/]+)$")) .. ".annotations.lua")
os.remove(dest)
stock:onExportAnnotations(true)
assert(io.open(dest, "r") == nil, "stock exports on close by default")
G_reader_settings:saveSetting("annotations_export_on_closing", true)
for _, keep in ipairs({false, true}) do
    G_reader_settings:saveSetting("device_id", "stock-reference-sender")
    stock:onExportAnnotations(true)
    local saved = Settings:open(dest)
    assert(saved:readSetting("device_id") == "stock-reference-sender")
    assert(saved:readSetting("paging") == nil)
    assert(#saved:readSetting("annotations") == 1)
    local receiver = reader(source, true, {unpack(baseline)})
    assert(receiver:importAnnotations() == nil, "same device must not import")
    G_reader_settings:saveSetting("device_id", "stock-reference-receiver")
    G_reader_settings:saveSetting("annotations_export_keep_all_on_import", keep)
    assert(receiver:importAnnotations() == true)
    assert(receiver.annotations[#receiver.annotations].note == "The updated note")
    assert(#receiver.annotations == (keep and 3 or 2))
    assert(io.open(dest, "r") == nil, "stock consumes an imported transfer")
end

local exported = arg[4]
if exported and exported ~= "" then
    local document = Cre.newDocView(1264, 1606, 1)
    document:setStyleSheet("./data/epub.css", "")
    document:setVisiblePageCount(1, false)
    document:setFontFace("Noto Sans")
    assert(document:loadDocument(source, false))
    document:renderDocument()
    local cre = setmetatable({ _document = document }, { __index = CREDocument })
    local wire = Settings:open(exported)
    local annotations = assert(wire:readSetting("annotations"))
    assert(#annotations == 2)
    local checked = 0
    for _, mark in ipairs(annotations) do
        assert(cre:isXPointerInDocument(mark.page), mark.page)
        if mark.drawer then
            assert(cre:isXPointerInDocument(mark.pos0), mark.pos0)
            assert(cre:isXPointerInDocument(mark.pos1), mark.pos1)
            local selected = cre:getTextFromXPointers(mark.pos0, mark.pos1, false)
            local text = type(selected) == "table" and selected.text or selected
            assert(text == mark.text, ("KOReader selected %q"):format(tostring(text)))
            assert(mark.note == 'Steel & blue — keep "this".\nUse the second shelf.')
            checked = checked + 1
        end
    end
    assert(checked == 1)
    document:close()
    print("Native CREngine resolved both Reader locations and selected the exact quotation")
end
print("Stock annotation exchange verified: default path, custom path, close opt-in, keep-all, edit dates, same device, and file consumption")

if arg[5] then
    local provider = require("document/pdfdocument")
    local fixed = assert(require("document/documentregistry"):openDocument(assert(arg[6]), provider))
    local wire = Settings:open(arg[5])
    assert(wire:isTrue("paging"))
    local annotations = assert(wire:readSetting("annotations"))
    assert(#annotations == 2)
    local checked = 0
    for _, mark in ipairs(annotations) do
        assert(mark.page == 1)
        if mark.drawer then
            local selected = fixed:getTextFromPositions(mark.pos0, mark.pos1)
            assert(selected and selected.text == mark.text, selected and selected.text)
            assert(mark.note == "Check the chart")
            assert(#mark.pboxes == 1 and #selected.pboxes == 1)
            local authored, provider = mark.pboxes[1], selected.pboxes[1]
            assert(math.abs(authored.x - provider.x) < 1)
            assert(math.abs(authored.w - provider.w) < 1)
            -- MuPDF's box follows the line ascent and descent. Require the
            -- same source span, with overlapping vertical font bounds.
            local overlap = math.min(authored.y + authored.h, provider.y + provider.h)
                - math.max(authored.y, provider.y)
            assert(overlap >= 0.6 * math.max(authored.h, provider.h))
            checked = checked + 1
        end
    end
    assert(checked == 1)
    fixed:close()
    print("Stock MuPDF resolved both fixed marks and selected the exact quotation")
end
