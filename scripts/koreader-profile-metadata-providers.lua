-- Run from the pinned stock runtime with ./luajit. The inputs are authored by
-- the Rust opening_metadata host tests using READER_METADATA_STOCK_FIXTURES.
local fixtures, font, cache = assert(arg[1]), assert(arg[2]), assert(arg[3])
require("setupkoenv")
local cre = require("libs/libkoreader-cre")
cre.initCache(cache, 0, true, 1)
cre.registerFont(font)

local function reflow(name)
    local view = cre.newDocView(1264, 1606, 1)
    view:setStyleSheet("./data/epub.css")
    view:setFontFace("Noto Sans")
    assert(view:loadDocument(fixtures .. "/" .. name, false))
    local metadata = view:getDocumentProps()
    view:close()
    return metadata
end

local function fixed(engine, name)
    local book = engine.openDocument(fixtures .. "/" .. name, false)
    local metadata = book:getMetadata()
    book:close()
    return metadata
end

local epub = reflow("Publisher metadata.epub")
assert(epub.title == "ĐẶNG [A+B]" and epub.authors == "Ada Scout" and epub.language == "vi")
assert(epub.series == "Sounding Log #2.5" and epub.description:find("Lower river delta", 1, true))
assert(epub.keywords == "Estuary\nCirculation")
print("CREngine EPUB: title, author, language, series, description and two ordered subjects")
local untitled = reflow("Seabird Survey.epub")
assert(untitled.title == "" and untitled.authors == "")
print("CREngine untitled EPUB: no provider title or author")

local mupdf = require("ffi/mupdf")
local pdf = fixed(mupdf, "Survey print.pdf")
assert(pdf.title == "Tide log" and pdf.author == "Ada Scout")
assert(pdf.subject == "Two coastal transects" and pdf.keywords == "Gauge, Shore")
assert(pdf.language == nil and pdf.series == nil)
print("MuPDF PDF: title, author, subject and keywords; catalog language unexposed")
local comic = fixed(mupdf, "Drawn sequence.cbz")
assert(comic.title == "" and comic.author == "" and comic.keywords == "")
print("MuPDF CBZ: no provider title, author or keywords")

local djvu = fixed(require("libs/libkoreader-djvu"), "Scanned sequence.djvu")
for name, expected in pairs({
    Title = "Beacon Atlas", Author = "Ada Scout", Series = "North archive #2.5",
    Language = "en", Keywords = "Beacon, Ice", Subject = "A weathered scan",
}) do
    assert((djvu[name] or djvu[name:lower()]) == expected, name)
end
print("DjVuLibre: title, author, series with decimal index, language, keywords and subject")
