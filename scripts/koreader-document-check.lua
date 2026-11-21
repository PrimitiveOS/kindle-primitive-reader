-- Read-only comparator for the installed KOReader providers on an Oasis.
-- From the KOReader installation, run ./luajit this-script scratch provider document.
-- The engine paints into memory and stores its comparison cache in scratch.
local scratch, provider, document = arg[1], arg[2], arg[3]
assert(scratch and scratch:sub(1,1) == "/" and scratch ~= "/mnt/us/documents" and scratch:sub(1,18) ~= "/mnt/us/documents/" and not scratch:find("/%.%./"), "Use an absolute scratch directory outside Documents")
assert(provider == "cre" or provider == "mupdf", "Choose a document engine")
assert(document and document:sub(1,18) == "/mnt/us/documents/" and not document:find("/%.%./"), "Choose a Kindle Documents file")
os.setlocale("C", "numeric")
require("setupkoenv")
local lfs = require("libs/libkoreader-lfs")
assert(lfs.symlinkattributes(document, "mode") == "file" and lfs.symlinkattributes(scratch, "mode") == "directory")
local time = require("ui/time")
local width, height = 1264, 1606
local started = time.monotonic()
local function ms() return time.to_ms(time.monotonic() - started) end
local function run()
    if provider == "mupdf" then
        local pdf = require("ffi/mupdf").openDocument(document)
        if pdf:needsPassword() then pdf:close(); return false end
        local count = pdf:getPages()
        assert(count >= 1)
        local opened = ms()
        local function paint(n)
            local page = pdf:openPage(n)
            local dc = require("ffi/drawcontext").new()
            local w, h = page:getSize(dc)
            dc:setZoom(math.min(width/w, height/h))
            local bb = page:draw_new(dc, width, height, 0, 0)
            bb:free()
            page:close()
        end
        paint(1)
        local first = ms()
        if count > 1 then paint(2) end
        pdf:close()
        return true, count, opened, first, ms() - first, "mupdf"
    end
    local cre = require("libs/libkoreader-cre")
    local cache = scratch .. "/comparison-cre-cache"
    if not lfs.attributes(cache) then assert(lfs.mkdir(cache)) end
    cre.initCache(cache, 0, true, 1)
    cre.registerFont("/mnt/us/launcher/fonts/NotoSans-Regular.ttf")
    local view = cre.newDocView(width, height, 1)
    view:setStyleSheet("./data/epub.css")
    view:setFontFace("Noto Sans")
    view:setFontSize(42)
    view:setDefaultInterlineSpace(135)
    view:setPageMargins(54, 54, 54, 54)
    view:setVisiblePageCount(1, false)
    view:setHeaderInfo(0)
    if not view:loadDocument(document, false) then view:close(); return false end
    view:renderDocument()
    local count = view:getPages()
    assert(count >= 1)
    local opened = ms()
    local BB = require("ffi/blitbuffer")
    local bb = BB.new(width, height, BB.TYPE_BB8)
    view:gotoPage(1)
    view:drawCurrentPage(bb, false, false, false, false)
    local first = ms()
    if count > 1 then
        view:gotoPage(2)
        view:drawCurrentPage(bb, false, false, false, false)
    end
    local next_ms = ms() - first
    local format = view:getDocumentFormat() or "unknown"
    local inspected
    if os.getenv("READER_COMPARE_CONTENTS") == "1" then
        local pages = { [1] = true, [count] = true }
        for _, entry in ipairs(view:getToc()) do
            if type(entry.page) == "number" and entry.page >= 1 and entry.page <= count then
                pages[entry.page] = true
            end
        end
        inspected = 0
        for page = 1, count do
            if pages[page] then
                view:gotoPage(page)
                view:drawCurrentPage(bb, false, false, false, false)
                inspected = inspected + 1
            end
        end
    end
    bb:free()
    view:close()
    -- Only an engine's format label may enter a public aggregate.
    local label = format:gsub("[^%w /+.-]", "")
    return true, count, opened, first, next_ms, label, inspected
end
local ok, opened, count, parse, first, turn, format, inspected = pcall(run)
if ok and opened then
    local extra = inspected and string.format(',"contents_pages_painted":%d', inspected) or ""
    print(string.format('{"opened":true,"provider":"%s","format":"%s","pages":%d,"parse_ms":%d,"first_frame_ms":%d,"next_ms":%d%s}',provider,format,count,parse,first,turn,extra))
else
    print(string.format('{"opened":false,"provider":"%s","outcome":"%s"}',provider,ok and "declined" or "engine error"))
end
