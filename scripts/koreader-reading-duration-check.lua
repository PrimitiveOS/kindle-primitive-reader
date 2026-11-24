-- Run inside the disposable profile prepared by koreader-reading-progress-check.py.
local UIManager = require("ui/uimanager")
local ReaderUI = require("apps/reader/readerui")
local Screen = require("device").screen
local DataStorage = require("datastorage")
local settings = assert(os.getenv("QA_READING_PROGRESS_SETTINGS"))
local frame = assert(os.getenv("QA_READING_PROGRESS_FRAME"))
assert(DataStorage:getSettingsDir() == settings, "Use the isolated profile from the companion runner")

local function top()
    local stack = UIManager._window_stack
    return stack[#stack] and stack[#stack].widget
end

local function control(widget, label)
    for _, row in ipairs(widget.layout or widget.buttons_layout) do
        for _, button in ipairs(row) do
            if button.text and button.text:find(label, 1, true) == 1 then
                return button
            end
        end
    end
    error("Missing widget control " .. label)
end

local function check_accounting(stats)
    local real_time = os.time
    local now = real_time()
    local old_stats, old_page = stats.page_stat, stats.curr_page
    local old_time, old_pages, old_turns = stats.mem_read_time, stats.mem_read_pages, stats.pageturn_count
    local old_min, old_max = stats.settings.min_sec, stats.settings.max_sec
    local function duration(minimum, maximum, elapsed)
        os.time = function() return now end
        stats.settings.min_sec, stats.settings.max_sec = minimum, maximum
        stats.curr_page = 1
        stats.page_stat = { [1] = { { now - elapsed, 0 } } }
        stats:onPageUpdate(false)
        os.time = real_time
        return stats.page_stat[1][1][2]
    end
    assert(duration(5, 120, 4) == 0)
    assert(duration(5, 120, 5) == 5)
    assert(duration(0, 7200, 1) == 1)
    assert(duration(0, 7200, 8000) == 7200)
    assert(duration(120, 10, 10) == 0)
    assert(duration(120, 10, 11) == 10)
    stats.page_stat, stats.curr_page = old_stats, old_page
    stats.mem_read_time, stats.mem_read_pages, stats.pageturn_count = old_time, old_pages, old_turns
    stats.settings.min_sec, stats.settings.max_sec = old_min, old_max
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
    local menu = {}
    stats:addToMainMenu(menu)
    local settings_menu
    for _, item in ipairs(menu.statistics.sub_item_table) do
        if item.text == "Settings" then settings_menu = item.sub_item_table; break end
    end
    local row = assert(settings_menu)[1]
    assert(row.text_func():find("Read page duration limits", 1, true) == 1 and row.keep_menu_open)
    local updates = 0
    local touchmenu = { updateItems = function() updates = updates + 1 end }
    row.callback(touchmenu)
    local widget = top()
    assert(widget.title_text == "Read page duration limits" and widget.is_range)
    assert(widget.left_min == 0 and widget.left_max == 120 and widget.left_default == 5)
    assert(widget.right_min == 10 and widget.right_max == 7200 and widget.right_default == 120)
    assert(widget.left_step == 1 and widget.left_hold_step == 10)
    assert(widget.right_step == 10 and widget.right_hold_step == 60)
    widget:onDoubleSpinButtonPressed({true, 1, false})
    widget:onDoubleSpinButtonPressed({true, 1, true})
    widget:onDoubleSpinButtonPressed({false, 1, false})
    widget:onDoubleSpinButtonPressed({false, 1, true})
    assert(widget.left_widget:getValue() == 16 and widget.right_widget:getValue() == 190)
    control(widget, "Close").callback()
    assert(updates == 0 and stats.settings.min_sec == 5 and stats.settings.max_sec == 120)
    row.callback(touchmenu)
    widget = top()
    widget.left_widget.text_value.callback()
    local keyboard = top()
    assert(keyboard.input_type == "number")
    keyboard.button_table:getButtonById("close").callback()
    assert(top() == widget)
    widget.left_widget.text_value.callback()
    keyboard = top()
    keyboard:setInputText("120")
    control(keyboard.button_table, "OK").callback()
    assert(top() == widget and widget.left_widget:getValue() == 120)
    widget.right_widget.text_value.callback()
    keyboard = top()
    keyboard:setInputText("10")
    control(keyboard.button_table, "OK").callback()
    assert(top() == widget and widget.right_widget:getValue() == 10)
    control(widget, "Default values").callback()
    assert(widget.left_widget:getValue() == 5 and widget.right_widget:getValue() == 120)
    widget:update(0, 7200)
    widget:onDoubleSpinButtonPressed({true, -1, true})
    widget:onDoubleSpinButtonPressed({false, 1, true})
    assert(widget.left_widget:getValue() == 0 and widget.right_widget:getValue() == 7200)
    UIManager:scheduleIn(0.25, function()
        assert(top() == widget and Screen:getWidth() == 1264 and Screen:getHeight() == 1680)
        Screen:shot(frame)
        control(widget, "Apply").callback()
        assert(top() == ui and updates == 1)
        assert(stats.settings.min_sec == 0 and stats.settings.max_sec == 7200)
        check_accounting(stats)
        print("Stock duration editor: defaults, tap/hold, numeric keypad, bounds, cancel, apply and six accounting cases")
        UIManager:quit(0)
    end)
end

UIManager:scheduleIn(0.5, check)
