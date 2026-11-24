-- The companion host command runs this patch with an authored fixed-page book and a fresh KO_HOME.
local UI = require("ui/uimanager")
local ReaderUI = require("apps/reader/readerui")
local SQ3 = require("lua-ljsqlite3/init")
local DataStorage = require("datastorage")
local settings = assert(os.getenv("QA_FIXED_STATS_SETTINGS"))
assert(DataStorage:getSettingsDir() == settings, "Use the isolated statistics fixture profile")
local djvu = os.getenv("QA_FIXED_STATS_FORMAT") == "djvu"
local expected_pages = djvu and 6 or 2
local turns = djvu and {{3, 12}, {6, 18}, {3, 30}} or {{2, 12}, {1, 18}}
local final_seconds = djvu and 15 or 30
local total = djvu and 75 or 60
local distinct = djvu and 3 or 2

local steps = 0
local function ready()
    steps = steps + 1
    local reader = ReaderUI.instance
    local stats = reader and reader.statistics
    if not (stats and stats.id_curr_book and stats.is_doc_not_frozen) then
        assert(steps < 40, "Stock statistics did not load for the authored fixed-page book")
        return UI:scheduleIn(0.25, ready)
    end
    assert(reader.paging and stats.data.pages == expected_pages and #stats.doc_md5 == 32)
    assert(not not reader.document.is_djvu == djvu)
    assert(reader.paging.current_page == 1)
    local original = os.time
    local now = 1769990394 -- 2026-02-01 23:59:54 UTC
    os.time = function(input) return input and original(input) or now end
    stats.page_stat = { [1] = {{now, 0}} }
    stats.curr_page = 1
    for _, turn in ipairs(turns) do
        now = now + turn[2]
        reader.paging:onGotoPage(turn[1])
        assert(reader.paging.current_page == turn[1] and stats.curr_page == turn[1])
    end
    now = now + final_seconds
    stats:onPageUpdate(false)
    stats:insertDB()
    os.time = original
    stats.is_doc_not_frozen = false -- avoid adding the emulator shutdown as a fixture visit

    local db = SQ3.open(settings .. "/statistics.sqlite3")
    assert(tonumber(db:rowexec("PRAGMA user_version")) == 20221111)
    assert(tonumber(db:rowexec("SELECT count(*) FROM page_stat_data")) == #turns + 1)
    assert(tonumber(db:rowexec("SELECT sum(duration) FROM page_stat")) == total)
    assert(tonumber(db:rowexec("SELECT count(DISTINCT page) FROM page_stat")) == distinct)
    db:exec("PRAGMA wal_checkpoint(TRUNCATE)")
    db:close()
    print(("Stock physical paging and insertDB produced %d records, %d seconds, %d distinct pages")
        :format(#turns + 1, total, distinct))
    UI:quit(0)
end

UI:scheduleIn(0.25, ready)
