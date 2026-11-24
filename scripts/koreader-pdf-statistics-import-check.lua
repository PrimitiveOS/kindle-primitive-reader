-- The companion host command runs this patch with an authored PDF and a fresh KO_HOME.
local UI = require("ui/uimanager")
local ReaderUI = require("apps/reader/readerui")
local SQ3 = require("lua-ljsqlite3/init")
local DataStorage = require("datastorage")
local settings = assert(os.getenv("QA_PDF_SQLITE_SETTINGS"))
assert(DataStorage:getSettingsDir() == settings, "Use the isolated statistics fixture profile")

local steps = 0
local function ready()
    steps = steps + 1
    local reader = ReaderUI.instance
    local stats = reader and reader.statistics
    if not (stats and stats.id_curr_book and stats.is_doc_not_frozen) then
        assert(steps < 40, "Stock statistics did not load for the authored PDF")
        return UI:scheduleIn(0.25, ready)
    end
    assert(reader.paging and stats.data.pages == 2 and #stats.doc_md5 == 32)
    local original = os.time
    local now = 1769990394 -- 2026-02-01 23:59:54 UTC
    os.time = function(input) return input and original(input) or now end
    stats.page_stat = { [1] = {{now, 0}} }
    stats.curr_page = 1
    now = now + 12
    stats:onPageUpdate(2)
    now = now + 18
    stats:onPageUpdate(1)
    now = now + 30
    stats:onPageUpdate(false)
    stats:insertDB()
    os.time = original
    stats.is_doc_not_frozen = false -- avoid adding the emulator shutdown as a fixture visit

    local db = SQ3.open(settings .. "/statistics.sqlite3")
    assert(tonumber(db:rowexec("PRAGMA user_version")) == 20221111)
    assert(tonumber(db:rowexec("SELECT count(*) FROM page_stat_data")) == 3)
    assert(tonumber(db:rowexec("SELECT sum(duration) FROM page_stat")) == 60)
    assert(tonumber(db:rowexec("SELECT count(DISTINCT page) FROM page_stat")) == 2)
    db:exec("PRAGMA wal_checkpoint(TRUNCATE)")
    db:close()
    print("Stock insertDB produced 3 PDF records, 60 seconds, 2 distinct pages")
    UI:quit(0)
end

UI:scheduleIn(0.25, ready)
