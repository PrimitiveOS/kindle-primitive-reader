-- Run the pinned TOC and header callbacks against an authored outline.
-- Usage: lua scripts/koreader-header-marks-check.lua <readertoc.lua> <readerrolling.lua>
local function load_function(path, name, declaration)
    local input = assert(io.open(path))
    local source = input:read("*a")
    input:close()
    local start = assert(source:find("function " .. name .. "(", 1, true))
    local finish = assert(source:find("\nend", start, true))
    local code = source:sub(start, finish + 3)
    assert(code:find(declaration, 1, true))
    assert(loadstring(code))()
end

ReaderToc = {}
ReaderRolling = {}
load_function(assert(arg[1]), "ReaderToc:getTocTicks", "ReaderToc:getTocTicks(level)")
load_function(arg[1], "ReaderToc:getTocTicksFlattened", "ReaderToc:getTocTicksFlattened(for_chapter_navigation)")
load_function(assert(arg[2]), "ReaderRolling:onUpdateTopStatusBarMarkers", "ReaderRolling:onUpdateTopStatusBarMarkers()")

local toc = setmetatable({
    toc = {
        { depth = 1, page = 11 },
        { depth = 2, page = 7 },
        { depth = 1, page = 1 },
        { depth = 2, page = 11 },
    },
    toc_ticks_ignored_levels = {},
    fillToc = function() end,
}, { __index = ReaderToc })
local captured
local rolling = setmetatable({
    cre_top_bar_enabled = true,
    ui = {
        toc = toc,
        document = {
            getPageCount = function() return 13 end,
            setHeaderProgressMarks = function(_, pages, ticks)
                captured = {pages = pages, ticks = table.concat(ticks, ",")}
            end,
        },
    },
}, { __index = ReaderRolling })
rolling:onUpdateTopStatusBarMarkers()
assert(captured.pages == 13 and captured.ticks == "1,7,11")
toc.toc_ticks_ignored_levels[2] = true
rolling:onUpdateTopStatusBarMarkers()
assert(captured.pages == 13 and captured.ticks == "1,11")
captured = nil
rolling.cre_top_bar_enabled = false
rolling:onUpdateTopStatusBarMarkers()
assert(captured == nil)
print("Stock header uses whole-book pages and sorted, deduplicated selected TOC depths")
