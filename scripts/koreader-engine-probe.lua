-- Run with the installed Kindle KOReader's LuaJIT from its installation directory.
-- ./luajit /mnt/us/primitiveos-reader-p0.RUN/koreader-engine-probe.lua /mnt/us/primitiveos-reader-p0.RUN prose.epub 30
-- The fixture EPUB and engine cache stay in the dedicated Reader probe directory.

local scratch, fixture = arg[1], arg[2]
local turns = tonumber(arg[3] or "30")
assert(scratch and scratch:match("^/mnt/us/primitiveos%-reader%-p0%.[%w]+$"),
    "Use a scratch directory created with mktemp -d /mnt/us/primitiveos-reader-p0.XXXXXX")
assert(fixture and fixture:match("^[%w_%-][%w_.%-]*%.epub$"), "Provide the scratch EPUB filename")
assert(turns and turns >= 0 and turns <= 1000 and turns % 1 == 0 and not arg[4],
    "Choose between 0 and 1000 warm turns")
os.setlocale("C", "numeric")

require("setupkoenv")
local lfs = require("libs/libkoreader-lfs")
assert(lfs.symlinkattributes(scratch, "mode") == "directory", "The scratch directory is unavailable")
local book = scratch .. "/" .. fixture
assert(lfs.symlinkattributes(book, "mode") == "file", "Copy the test EPUB into the scratch directory")

-- Each process uses an empty cache so repeated runs leave the installed cache alone.
local proc_stat = assert(io.open("/proc/self/stat"))
local pid = assert(proc_stat:read("*n"))
proc_stat:close()
local cache = scratch .. "/cre-cache-" .. pid
assert(lfs.mkdir(cache))

local time = require("ui/time")
local width, height = 1264, 1606
local start = time.monotonic()
local cre = require("libs/libkoreader-cre")
local Blitbuffer = require("ffi/blitbuffer")
cre.initCache(cache, 0, true, 1)
cre.registerFont("/var/tmp/kl/fonts/NotoSans-Regular.ttf")

local view = cre.newDocView(width, height, 1)
view:setStyleSheet("./data/epub.css")
view:setFontFace("Noto Sans")
assert(view:getFontFace() == "Noto Sans", "The baseline reading font is unavailable")
view:setFontSize(42)
view:setDefaultInterlineSpace(135)
view:setPageMargins(54, 54, 54, 54)
view:setVisiblePageCount(1, false)
view:setHeaderInfo(0)
assert(view:loadDocument(book, false), "CREngine could not load the test EPUB")
view:renderDocument()
local pages = view:getPages()
assert(pages > 0, "CREngine produced no pages")

local paint_start = time.monotonic()
local buffer = Blitbuffer.new(width, height, Blitbuffer.TYPE_BB8)
view:gotoPage(1)
view:drawCurrentPage(buffer, false, false, false, false)
local painted = time.monotonic()

local samples = {}
for index = 1, turns do
    local turn_start = time.monotonic()
    view:gotoPage(1 + index % pages)
    view:drawCurrentPage(buffer, false, false, false, false)
    samples[index] = time.to_ms(time.monotonic() - turn_start)
end
table.sort(samples)

local function percentile(fraction)
    if #samples == 0 then
        return "null"
    end
    return string.format("%.3f", samples[math.ceil(#samples * fraction)])
end

local peak = "null"
local status = io.open("/proc/self/status")
if status then
    peak = status:read("*a"):match("VmHWM:%s+(%d+)") or peak
    status:close()
end

print(string.format(
    '{"engine":"koreader-cre","layout_scope":"whole-book","fixture":"%s","pages":%d,"layout_ms":%.3f,' ..
    '"first_paint_ms":%.3f,"first_frame_ms":%.3f,"warm_rasterization_count":%d,' ..
    '"warm_rasterization_median_ms":%s,"warm_rasterization_p95_ms":%s,"peak_rss_kib":%s}',
    fixture, pages, time.to_ms(paint_start - start), time.to_ms(painted - paint_start),
    time.to_ms(painted - start), turns, percentile(0.5), percentile(0.95), peak))

view:close()
buffer:free()
