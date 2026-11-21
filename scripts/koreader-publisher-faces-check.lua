-- Read one original in an isolated cache. Only aggregate face counts and a
-- comparison at a shared source location leave the process.
local scratch, book, inventory = arg[1], arg[2], arg[3]
assert(scratch and book and (not inventory or inventory == "inventory") and not arg[4])
os.setlocale("C", "numeric")
require("setupkoenv")
local lfs = require("libs/libkoreader-lfs")
local cre = require("libs/libkoreader-cre")
local BB = require("ffi/blitbuffer")
local md5 = require("ffi/sha2").md5
assert(lfs.symlinkattributes(scratch, "mode") == "directory")
local cache = scratch .. "/publisher-face-cache"
if not lfs.attributes(cache) then assert(lfs.mkdir(cache)) end
assert(lfs.symlinkattributes(cache, "mode") == "directory")
cre.initCache(cache, 0, true, 1)
cre.registerFont("/mnt/us/launcher/fonts/NotoSans-Regular.ttf")
local function render(enabled, anchor)
    local view = cre.newDocView(600, 800, 1)
    view:setStyleSheet("./data/epub.css")
    view:setIntProperty("crengine.doc.embedded.fonts.enabled", enabled)
    view:setIntProperty("crengine.doc.embedded.styles.enabled", 1)
    view:setFontFace("Noto Sans")
    view:setFontSize(32)
    view:setPageMargins(24, 24, 24, 24)
    view:setHeaderInfo(0)
    assert(view:loadDocument(book, false))
    view:renderDocument()
    local pages = view:getPages()
    assert(pages >= 3)
    anchor = anchor or view:getPageXPointer(math.floor(pages / 2))
    local page = view:getPageFromXPointer(anchor)
    assert(page >= 1 and page <= pages)
    view:gotoPage(page)
    local frame = BB.new(600, 800, BB.TYPE_BB8)
    view:drawCurrentPage(frame,false,false,false,false)
    local fingerprint = md5(BB.tostring(frame))
    frame:free()
    local active, registered = 0, 0
    for _, used in pairs(view:getEmbeddedFontList()) do
        registered = registered + 1
        if used then active = active + 1 end
    end
    view:close()
    return fingerprint, active, anchor, registered
end
local enabled, active, anchor, registered = render(1)
local disabled = render(0, anchor)
local restored = render(1, anchor)
local changed, stable = enabled ~= disabled, enabled == restored
print(string.format("publisher_stock instantiated=%d embedded_toggle_changes=%s source_restores=%s registered=%d",
    active, tostring(changed), tostring(stable), registered))
assert(stable)
if not inventory then assert(active >= 1 and changed) end
