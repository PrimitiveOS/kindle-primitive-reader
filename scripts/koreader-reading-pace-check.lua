-- Run in the stock KOReader directory with ./luajit. All SQL uses an in-memory database.
-- Arguments: scratch, real document, physical/CREngine page, Reader chapter pages left,
-- Reader book pages left. The latter two compare the pace formula independently of layout.
local scratch, path = arg[1], arg[2]
local page, reader_chapter, reader_book = tonumber(arg[3]), tonumber(arg[4]), tonumber(arg[5])
assert(scratch and scratch:match("^/mnt/us/%.primitive%-reader%-parity%.[%w]+/[%w%-]+/[%w%-]+$"))
assert(path and path:sub(1,18) == "/mnt/us/documents/" and not path:find("/%.%./"))
assert(page and page >= 1 and reader_chapter and reader_book)
require("setupkoenv")
local lfs = require("libs/libkoreader-lfs")
assert(lfs.symlinkattributes(scratch, "mode") == "directory" and lfs.symlinkattributes(path, "mode") == "file")

local function source(name)
    local f = assert(io.open(name, "rb"))
    local text = f:read("*all")
    f:close()
    return text
end

-- Use the implementation on the device while keeping document metadata, settings and
-- clock formatting outside the comparison. Nothing instantiates the statistics plugin.
local function method(text, signature, environment)
    local a = assert(text:find("function " .. signature .. "(", 1, true))
    local b = assert(text:find("\nend", a, true))
    local chunk = text:sub(a, b+3):gsub("^function [^(]+%(", "return function(self, ", 1)
    local load = assert(loadstring(chunk))
    setfenv(load, environment)
    return load()
end

local stats = source("plugins/statistics.koplugin/main.lua")
local sql = assert(stats:match("local STATISTICS_SQL_BOOK_CAPPED_TOTALS_QUERY = %[%[(.-)%]%]"))
local db = require("lua-ljsqlite3/init").open(":memory:")
db:exec("CREATE TABLE page_stat(id_book INTEGER, page INTEGER, duration INTEGER)")
db:exec("INSERT INTO page_stat VALUES (1,1,20),(1,2,40),(1,2,10),(2,1,120)")
local visited, seconds = db:rowexec(string.format(sql, 120, 1))
db:close()
visited, seconds = tonumber(visited), tonumber(seconds)
assert(visited == 2 and seconds == 70)
local rate = seconds / visited
local time_left = method(stats, "ReaderStatistics:getTimeForPages", {
    G_reader_settings = {readSetting = function(_, _, default) return default end},
    datetime = {secondsToClockDuration = function(_, value) return value end},
})
local timer = {settings = {is_enabled = true}, avg_time = rate}
local count, toc
local matched = false
if path:lower():match("%.pdf$") then
    local pdf = require("ffi/mupdf").openDocument(path)
    count, toc = pdf:getPages(), pdf:getToc()
    pdf:close()
else
    local cre = require("libs/libkoreader-cre")
    cre.initCache(scratch, 0, true, 1)
    cre.registerFont("/mnt/us/launcher/fonts/NotoSans-Regular.ttf")
    local view = cre.newDocView(1264, 1606, 1)
    view:setStyleSheet("./data/epub.css")
    view:setFontFace("Noto Sans")
    view:setFontSize(42)
    view:setDefaultInterlineSpace(135)
    view:setPageMargins(54, 54, 54, 54)
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    assert(view:loadDocument(path, false))
    view:renderDocument()
    count, toc = view:getPages(), view:getToc()
    local passages = assert(io.open(scratch .. "/passages", "rb"))
    for snippet in passages:lines() do
        local hits = view:findAllText(snippet, false, false, 2, true, 0, 255) or {}
        if #hits == 1 and hits[1].start then
            page = view:getPageFromXPointer(hits[1].start)
            matched = true
            break
        end
    end
    passages:close()
    assert(matched, "Could not identify a unique passage at the Reader location")
    view:close()
end
assert(count >= page)
local next_page
for _, entry in ipairs(toc) do
    if entry.page and entry.page > page and (not next_page or entry.page < next_page) then
        next_page = entry.page
    end
end
local doc = {info = {number_of_pages = count}, hasHiddenFlows = function() return false end}
local chapter = {getNextChapter = function() return next_page end, ui = {document = doc}}
local book_left = method(source("frontend/document/document.lua"), "Document:getTotalPagesLeft", {})(doc, page)
local chapter_left = method(source("frontend/apps/reader/modules/readertoc.lua"), "ReaderToc:getChapterPagesLeft", {})(chapter, page, true) or book_left
print(string.format('{"rate_seconds":%d,"distinct_pages":%d,"provider_pages":%d,"provider_page":%d,"matched_text_anchor":%s,"chapter_remaining":%d,"book_remaining":%d,"chapter_seconds":%d,"book_seconds":%d,"reader_layout_chapter_seconds":%d,"reader_layout_book_seconds":%d}',
    rate, visited, count, page, tostring(matched), chapter_left, book_left, time_left(timer, chapter_left),
    time_left(timer, book_left), time_left(timer, reader_chapter), time_left(timer, reader_book)))
