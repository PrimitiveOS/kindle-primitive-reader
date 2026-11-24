-- Exercise the pinned KOReader reset menu and callbacks against a fake SQLite connection.
local file = assert(io.open(assert(arg[1])))
local source = file:read("*a")
file:close()
ReaderStatistics = {}
for _, name in ipairs({
    "genResetBookSubItemTable", "resetPerBook", "resetCurrentBook",
    "deleteBooksByTotalDuration", "resetStatsForBookForPeriod",
}) do
    local start = assert(source:find("function ReaderStatistics:" .. name .. "(", 1, true))
    local finish = assert(source:find("\nend", start, true))
    assert(loadstring(source:sub(start, finish + 3)))()
end

_ = function(text) return text end
N_ = function(one, many, count) return count == 1 and one or many end
T = function(text, value) return (text:gsub("%%1", tostring(value))) end
local shown, removed_row, deleted, bindings, duration_resets
UIManager = { show = function(_, item) shown = item end }
ConfirmBox = { new = function(_, item) return item end }
InfoMessage = ConfirmBox
KeyValuePage = ConfirmBox
G_reader_settings = { readSetting = function() return "classic" end }
datetime = { secondsToClockDuration = function(_, seconds) return tostring(seconds) end }
logger = { dbg = function() end }
local ordered = {8, 7, 9}
local db = {
    close = function() end,
    exec = function(_, sql)
        assert(sql:find("last_open DESC", 1, true))
        return { ordered, id = ordered }
    end,
    rowexec = function(_, sql)
        if sql:find("sum(duration)", 1, true) then return 23 end
        if sql:find("changes()", 1, true) then return 2 end
        local id = assert(sql:match("id = (%d+)"))
        return "Book " .. id
    end,
    prepare = function(_, sql)
        assert(sql:find("id != ?", 1, true) and sql:find("total_read_time < ?", 1, true))
        local stmt = {
            reset = function(self) return self end,
            bind = function(self, id, threshold)
                table.insert(bindings, { id, threshold })
                return self
            end,
            step = function(self) return self end,
            close = function() end,
        }
        return stmt
    end,
}
SQ3 = { open = function() return db end }
local statistics = setmetatable({
    id_curr_book = 7,
    settings = { max_sec = 120 },
    isEnabled = function() return true end,
    insertDB = function() end,
    deleteBook = function(_, id) table.insert(deleted, id) end,
    resetVolatileStats = function() duration_resets = duration_resets + 1 end,
    getIdBookDB = function() return 10 end,
}, { __index = ReaderStatistics })

deleted, bindings, duration_resets = {}, {}, 0
local menu = statistics:genResetBookSubItemTable()
assert(#menu == 7 and menu[1].enabled_func() == 7)
menu[2].callback()
assert(#shown.kv_pairs == 2 and shown.kv_pairs[1][3] == 8 and shown.kv_pairs[2][3] == 9)
local row = shown.kv_pairs[2]
row.callback({ removeKeyValueItem = function(_, item) removed_row = item end }, row)
shown.cancel_callback()
assert(#deleted == 0)
shown.ok_callback()
assert(#deleted == 1 and deleted[1] == 9 and removed_row == row)
for index, minutes in ipairs({1, 5, 15, 30, 60}) do
    bindings = {}
    menu[index + 2].callback()
    assert(#bindings == 0)
    shown.ok_callback()
    assert(#bindings == 2)
    for _, bind in ipairs(bindings) do
        assert(bind[1] == 7 and bind[2] == minutes * 60)
    end
end
menu[1].callback()
shown.ok_callback()
assert(deleted[2] == 7 and statistics.id_curr_book == 10)
assert(statistics.book_read_pages == 0 and statistics.book_read_time == 0)
assert(statistics.avg_time == 60 and duration_resets == 1)
print("Pinned statistics reset scopes, strict cutoffs, confirmation and current-book pace passed")

C_ = function(_, text) return text end
local writes, refreshes = 0, 0
local period_sql
local period_stmt = {
    reset = function(self) return self end,
    bind = function(self, id, first, last)
        assert(id == 8 and first == 100 and last == 199)
        return self
    end,
    step = function() writes = writes + 1 end,
    close = function() end,
}
db.prepare = function(_, sql)
    assert(sql:find("DELETE FROM page_stat_data", 1, true))
    assert(sql:find("id_book = ?", 1, true) and sql:find("start_time between ? and ?", 1, true))
    period_sql = sql
    return period_stmt
end
local function refresh() refreshes = refreshes + 1 end
statistics:resetStatsForBookForPeriod(8, 100, 199, nil, refresh)
assert(shown.text:find("Book 8", 1, true) and shown.ok_text == "Reset")
shown.cancel_callback()
assert(writes == 0 and refreshes == 0)
statistics:resetStatsForBookForPeriod(8, 100, 199, "2026-01-01", refresh)
assert(shown.text:find("2026-01-01", 1, true))
shown.ok_callback()
assert(writes == 1 and refreshes == 1)
if run_period_sql then run_period_sql(period_sql) end
print("Pinned period reset checks the selected book and dates, confirmation and refresh")
