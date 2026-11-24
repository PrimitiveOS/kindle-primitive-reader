-- Runs as a patch in the disposable native profile from the companion runner.
local UIManager = require("ui/uimanager")
local ReaderUI = require("apps/reader/readerui")
local Screen = require("device").screen
local DataStorage = require("datastorage")
assert(DataStorage:getSettingsDir() == assert(os.getenv("QA_READING_PROGRESS_SETTINGS")))
local frame = assert(os.getenv("QA_READING_PROGRESS_FRAME"))

local function top()
    return UIManager._window_stack[#UIManager._window_stack].widget
end

local function row(items, title)
    for _, candidate in ipairs(items) do
        local label = candidate.text or (candidate.text_func and candidate.text_func())
        if label and label:sub(1, #title) == title then return candidate end
    end
end

local function button(widget, name)
    for _, buttons in ipairs(widget.layout or widget.buttons_layout) do
        local item = row(buttons, name)
        if item then return item end
    end
    error("Missing control " .. name)
end

local checks = 0
local function check()
    local ui = ReaderUI.instance
    if not (ui and ui.statistics and top() == ui) then
        checks = checks + 1
        assert(checks < 40, "Statistics did not start")
        UIManager:scheduleIn(0.25, check)
        return
    end
    local stats = ui.statistics
    local menu = {}
    stats:addToMainMenu(menu)
    local options = assert(row(menu.statistics.sub_item_table, "Settings")).sub_item_table
    local count = assert(row(options, "Books per calendar day"))
    local time = assert(row(options, "Daily timeline starts at"))
    local shift = assert(row(options, "Also use in calendar view"))
    local updates = 0
    local context = {updateItems = function() updates = updates + 1 end}
    count.callback(context)
    local spin = top()
    assert(spin.value == 3 and spin.value_min == 1 and spin.value_max == 5)
    spin.value = 5
    spin.callback(spin)
    UIManager:close(spin)
    assert(stats.settings.calendar_nb_book_spans == 5 and updates == 1)

    local function edit_time()
        time.callback(context)
        local widget = top()
        assert(widget.hour == 0 and widget.min == 0)
        assert(widget.hour_widget.value_hold_step == 4 and widget.min_widget.value_max == 50)
        assert(widget.min_widget.value_step == 10 and widget.min_widget.value_hold_step == 30)
        widget:onDateTimeButtonPressed({"left_widget", 4})
        widget:onDateTimeButtonPressed({"right_widget", 1})
        widget:onDateTimeButtonPressed({"right_widget", 1})
        assert(widget.hour_widget:getValue() == 4 and widget.min_widget:getValue() == 20)
        return widget
    end
    local widget = edit_time()
    widget:onClose()
    assert(updates == 1 and time.text_func():find("00:00", 1, true))
    widget = edit_time()
    UIManager:scheduleIn(0.25, function()
        assert(top() == widget)
        Screen:shot(frame)
        button(widget, "Set time").callback()
        assert(updates == 2 and time.text_func():find("04:20", 1, true))
        assert(top() == ui and not shift.checked_func())
        shift.callback()
        assert(shift.checked_func())
        shift.callback()
        assert(not shift.checked_func() and stats.settings.calendar_day_start_minute == 20)
        print("Stock calendar options verified 1–5 book spans, 04:20 timeline, cancellation and independent calendar shift")
        UIManager:quit(0)
    end)
end

UIManager:scheduleIn(0.5, check)
