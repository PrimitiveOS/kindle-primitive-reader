-- Return the SQL and cutoff from the real pinned Time range callbacks.
local file = assert(io.open(assert(arg[1])))
local source = file:read("*a")
file:close()
ReaderStatistics = {}
local code = {}
for _, name in ipairs({"local function sqlDaily(", "local function sqlWeekly(",
    "local function sqlMonthly(", "function ReaderStatistics:onShowTimeRange(",
    "function ReaderStatistics:getDatesFromAll("}) do
    local start = assert(source:find(name, 1, true))
    local finish = assert(source:find("\nend", start, true))
    code[#code + 1] = source:sub(start, finish + 3)
end
assert(loadstring(table.concat(code, "\n")))()

local current
UIManager = { show = function(_, item) current = item end, close = function() end }
KeyValuePage = { new = function(_, item) return item end }
_ = function(text) return text end
G_reader_settings = { readSetting = function() return "classic" end }
local native_date = os.date
local now = 1767704453 -- 2026-01-06 13:00:53 UTC
os.time = function() return now end
os.date = function(format, time) return native_date(format, time or now) end
local pending_query
SQ3 = { open = function() return {
    exec = function(_, query) pending_query = query; return nil end,
    close = function() end,
} end }
local all_books = 0
local reader = setmetatable({
    insertDB = function() end,
    getTotalStats = function() all_books = all_books + 1; return "Authored books", {} end,
}, { __index = ReaderStatistics })
reader:onShowTimeRange()
local menu = current
local names = { "All books", "Books by week", "Books by month", "Last week",
    "Last month by day", "Last year by day", "Last year by week", "All stats by month" }
local days = { 0, 0, 0, 7, 30, 365, 365, 0 }
assert(#menu.kv_pairs == #names)
local queries = {}
for i, row in ipairs(menu.kv_pairs) do
    assert(row[1] == names[i])
    pending_query = nil
    row.callback()
    if i == 1 then
        assert(all_books == 1 and pending_query == nil)
    else
        local from = assert(tonumber(pending_query:match("start_time >= (%d+)")))
        assert(from == (days[i] == 0 and 0 or now - 46853 - (days[i] - 1) * 86400))
        queries[row[1]] = pending_query
    end
    current.callback_return()
    assert(current == menu)
end
return queries
