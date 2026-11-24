-- This patch is copied into an isolated emulator profile by the companion runner.
local UIManager = require("ui/uimanager")
local ReaderUI = require("apps/reader/readerui")
local Screen = require("device").screen
local SQ3 = require("lua-ljsqlite3/init")
local DataStorage = require("datastorage")
local settings = assert(os.getenv("QA_READING_PROGRESS_SETTINGS"))
local frame = assert(os.getenv("QA_READING_PROGRESS_FRAME"))
assert(DataStorage:getSettingsDir() == settings, "Use the isolated profile from the companion runner")

local function top()
    local stack = UIManager._window_stack
    return stack[#stack] and stack[#stack].widget
end

local function stamp(year, month, day, hour, min, sec)
    return os.time{year=year, month=month, day=day, hour=hour, min=min, sec=sec}
end

local attempts = 0
local function check()
    attempts = attempts + 1
    local ui = ReaderUI.instance
    if not (ui and ui.statistics and top() == ui) then
        assert(attempts < 40, "Statistics did not load in the isolated reader")
        UIManager:scheduleIn(0.25, check)
        return
    end
    local stats = ui.statistics
    local current_time, current_date = os.time, os.date
    local now = stamp(2026, 2, 2, 12, 0, 0)
    stats.start_current_period = stamp(2026, 2, 1, 23, 59, 50)
    stats.is_doc_not_frozen = false
    local database = SQ3.open(settings .. "/statistics.sqlite3")
    assert(tonumber(database:rowexec("SELECT count(*) FROM page_stat_data")) == 0, "Expected an empty fixture database")
    database:exec([[
        INSERT INTO book (id, title, authors, md5, pages) VALUES
            (100, 'Harbor notes', 'Reading fixture', 'fixture-a', 50),
            (101, 'Coast atlas', 'Reading fixture', 'fixture-b', 50);
    ]])
    for _, visit in ipairs({
        {100, 5, stamp(2026, 1, 26, 23, 59, 59), 99},
        {100, 5, stamp(2026, 1, 27, 0, 0, 0), 20},
        {100, 5, stamp(2026, 1, 29, 19, 0, 0), 40},
        {100, 5, stamp(2026, 1, 29, 20, 0, 0), 15},
        {100, 8, stamp(2026, 2, 1, 23, 59, 55), 10},
        {100, 8, stamp(2026, 2, 2, 0, 0, 15), 60},
        {101, 8, stamp(2026, 2, 2, 11, 55, 0), 35},
        {100, 8, stamp(2026, 2, 2, 11, 59, 39), 15},
    }) do
        database:exec(string.format("INSERT INTO page_stat_data VALUES (%d, %d, %d, %d, 50)", unpack(visit)))
    end
    database:close()

    -- Keep SQL's notion of today and the chart's displayed dates on this fixture.
    os.time = function(input) return input and current_time(input) or now end
    os.date = function(format, input) return current_date(format, input or now) end
    stats:onShowReaderProgress()
    os.time, os.date = current_time, current_date
    local widget = top()
    assert(widget ~= ui and widget.dates and #widget.dates == 7, "The stock report did not open")
    local minutes, pages = widget:getTotalStats(7)
    assert(minutes == 195 and pages == 5)
    assert(widget.today_duration == 110 and tonumber(widget.today_pages) == 2)
    assert(widget.current_duration == 120 and tonumber(widget.current_pages) == 2)
    local known = { ["2026-02-02"] = 110, ["2026-02-01"] = 10, ["2026-01-29"] = 55, ["2026-01-27"] = 20 }
    for _, day in ipairs(widget.dates) do
        assert(day[2] == (known[day[3]] or 0))
    end
    UIManager:scheduleIn(0.25, function()
        assert(top() == widget and Screen:getWidth() == 1264 and Screen:getHeight() == 1680)
        Screen:shot(frame)
        widget:onSwipe(nil, {direction="east"})
        assert(top() == widget)
        widget:onSwipe(nil, {direction="south"})
        assert(top() == ui)
        print("Stock weekly progress: 7 dates, 195 seconds, 5 pages; today 110/2; session 120/2; close returned to the book")
        UIManager:quit(0)
    end)
end

UIManager:scheduleIn(0.5, check)
