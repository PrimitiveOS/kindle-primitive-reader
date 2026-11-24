-- Run with the installed KOReader's LuaJIT from its installation directory.
-- Load stock callbacks with in-memory settings, widgets, and fixed-page state.
local file = assert(io.open("frontend/apps/reader/modules/readersearch.lua", "rb"))
local source = file:read("*a")
file:close()

local function methods(first, last)
    local start = assert(source:find("function ReaderSearch:" .. first .. "(", 1, true))
    local finish = assert(source:find(last, start, true))
    return source:sub(start, finish - 1)
end

local callbacks = table.concat({
    "local ReaderSearch = {}",
    methods("init", "local regex_help_text"),
    methods("addToMainMenu", "-- if reverse == 1 search backwards"),
    methods("onShowSearchDialog", "-- if case == true or nil"),
    methods("updateAllResultsMenu", "function ReaderSearch:gotoResultsItem("),
    methods("onGoToStartPage", "return ReaderSearch"),
    "return ReaderSearch",
}, "\n")

local saved, shown = {}, {}
local settings = {}
function settings:readSetting(key) return saved[key] end
function settings:isTrue(key) return saved[key] == true end
function settings:saveSetting(key, value) saved[key] = value end
function settings:delSetting(key) saved[key] = nil end
function settings:flipNilOrFalse(key) saved[key] = not saved[key] end

local manager = {
    show = function(_, widget) shown = widget end,
    setDirty = function() end,
    close = function() end,
}
local widget = { new = function(_, spec) return spec end }
local function translate(text) return text end
local stock = assert(loadstring(callbacks))
setfenv(stock, setmetatable({
    G_reader_settings = settings,
    UIManager = manager,
    SpinWidget = widget,
    ButtonDialog = widget,
    Notification = widget,
    logger = { dbg = function() end },
    BD = { getArrowLabels = function() return "Previous", "Next", "First", "Last" end },
    _ = translate,
    T = function(pattern, value) return (pattern:gsub("%%1", tostring(value))) end,
}, { __index = _G }))

local entered, exited, cleared, directed, selected, goto_page = 0, 0, 0, 0, 0, nil
local fit, no_match = false, false
local highlight, paging = { temp = {} }, { current_page = 4 }
function paging:enterSkimMode()
    entered = entered + 1
    fit = true
end
function paging:exitSkimMode()
    assert(fit)
    exited = exited + 1
    fit = false
end
local reader = setmetatable({
    view = { highlight = highlight },
    ui = {
        highlight = { clear = function() cleared = cleared + 1 end },
        menu = { registerToMainMenu = function() end },
        paging = paging,
        link = { onGotoLink = function(_, location) goto_page = location.page + 1 end },
    },
    searchCallback = function(_, direction, text)
        assert(text == "owned fixture")
        if direction == 0 then
            directed = directed + 1
        else
            assert(direction == nil)
            selected = selected + 1
        end
    end,
    searchFromCurrent = function()
        assert(fit == settings:isTrue("fulltext_search_zoom_to_page"))
        if not no_match then return { page = 7 } end
    end,
    searchNext = function()
        assert(fit)
        return { page = 8 }
    end,
}, { __index = stock() })

reader:init()
local menu = {}
reader:addToMainMenu(menu)
local selection, context, max_lines, rows, zoom = unpack(menu.fulltext_search_settings.sub_item_table)
assert(reader.findall_nb_context_words == 5 and reader.findall_results_per_page == 10)
assert(not selection.checked_func() and not zoom.checked_func())
reader:searchText("owned fixture")
selection.callback()
reader:searchText("owned fixture")
assert(directed == 1 and selected == 1 and cleared == 1)
local refresh = { updateItems = function() end }
context.callback(refresh)
assert(shown.value_min == 1 and shown.value_max == 50 and shown.value_hold_step == 5)
assert(shown.value == 5 and shown.default_value == 5)
shown.callback({value = 50})
assert(saved.fulltext_search_nb_context_words == 50)
rows.callback(refresh)
assert(shown.value_min == 2 and shown.value_max == 24)
assert(shown.value == 10 and shown.default_value == 10)
shown.callback({value = 24})
assert(saved.fulltext_search_results_per_page == 24 and rows.enabled_func())
max_lines.callback(refresh)
assert(shown.value_min == 1 and shown.value_max == 10 and shown.value == 4)
shown.callback({value = 4})
assert(saved.fulltext_search_results_max_lines == 4 and not rows.enabled_func())
shown.extra_callback()
assert(rows.enabled_func() and reader.findall_results_per_page == 24)

reader:onShowSearchDialog("owned fixture", 0, {}, true)
assert(goto_page == 7 and highlight.temp[7] and entered == 0 and exited == 0)
zoom.callback()
reader:onShowSearchDialog("owned fixture", 0, {}, true)
assert(goto_page == 7 and fit and entered == 1 and exited == 0)
reader.search_dialog.buttons[1][4].callback()
assert(goto_page == 8 and highlight.temp[8] and entered == 1)
reader.search_dialog.tap_close_callback()
assert(not fit and exited == 1 and cleared == 2)
no_match = true
reader:onShowSearchDialog("owned fixture", 0, {}, true)
assert(not fit and entered == 2 and exited == 2)
-- Drive the options that the static stock screenshot inventory cannot open.
local all_results = {{start = 2}, {start = 5}, {start = 5}, {start = 9}}
local active_page, focused_index, closed = 5, nil, 0
local headings = {[2] = "First", [5] = "Middle", [9] = "Last"}
reader.start_page = 3
reader.findall_results = all_results
reader.result_menu = {
    item_table = all_results,
    switchItemTable = function(menu, _, items, index)
        if items then menu.item_table = items end
        focused_index = index
    end,
    close_callback = function() closed = closed + 1 end,
}
reader.ui.getCurrentPage = function() return active_page end
reader.ui.toc = {
    getTocTitleOfCurrentPage = function() return headings[active_page] end,
    getTocTitleByPage = function(_, page) return headings[page] end,
}
reader.ui.annotation = {getPageRef = function(_, _, page) return page end}
reader.ui.paging.onGotoPage = function(_, page) goto_page = page end

reader:showAllResultsMenuDialog()
shown.buttons[2][1].callback()
local filtered = reader.result_menu.item_table
assert(#filtered == 2 and filtered[1] == all_results[2] and filtered[2] == all_results[3])
reader:showAllResultsMenuDialog()
shown.buttons[1][1].callback()
assert(reader.result_menu.item_table == all_results)

reader:showAllResultsMenuDialog()
shown.buttons[4][1].callback()
assert(focused_index == 2 and active_page == 5)
active_page = 8
reader:showAllResultsMenuDialog()
shown.buttons[4][1].callback()
assert(focused_index == 3)
reader:showAllResultsMenuDialog()
shown.buttons[5][1].callback()
assert(closed == 1 and goto_page == 3)

print("stock-search selection=forward/list context=1..50/5 rows=2..24/10 max_lines=1..10/4 fit=off/enter/step/close/exhaust")
print("stock-search-results chapter=2/4 first_on_page=2 prior_between_pages=3 origin=3")
