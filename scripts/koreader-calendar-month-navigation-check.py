#!/usr/bin/env python3
"""Exercise the pinned KOReader month navigation and timeline return callbacks."""
import argparse
import hashlib
from pathlib import Path

from lupa.lua51 import LuaRuntime

PIN = "9192014d8bd82a91dc1012473be0f238dedfdb54"
CHECKSUM = "80e232d13a46660c2c0c9aafb2a043668c36175d57a8ee5b1539c583b00347b1"


def segment(source, begin, end):
    start = source.index(begin)
    return source[start:source.index(end, start + len(begin))]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("calendar", type=Path, help="pinned statistics.koplugin/calendarview.lua")
    args = parser.parse_args()
    data = args.calendar.read_bytes()
    if hashlib.sha256(data).hexdigest() != CHECKSUM:
        parser.error(f"Use the unmodified calendarview.lua from KOReader {PIN}")
    source = data.decode()
    controls = segment(
        source,
        "    self.page_info_left_chev = Button:new{",
        "    self.page_info = HorizontalGroup:new{",
    )
    months = segment(source, "function CalendarView:nextMonth()", "function CalendarView:onNextMonth()")
    timeline_close = segment(
        source, "                    close_callback = function(this)", "                    min_month = self.min_month,"
    )

    lua = LuaRuntime()
    lua.execute("""
        CalendarView = {}
        _ = function(text) return text end
        Button = {new = function(_, widget) return widget end}
        HorizontalSpan = Button
        Screen = {scaleBySize = function(_, size) return size end}
        UIManager = {
            show = function(self, widget) self.last = widget end,
            tickAfterNext = function(self, callback) self.queued = callback end,
        }
        package.preload["ui/widget/infomessage"] = function() return Button end
    """)
    lua.execute("function CalendarView:mountControls()\n" + controls + "\nend")
    lua.execute(months)
    lua.execute("function CalendarView:dayReturn() return ({" + timeline_close + "}).close_callback end")
    lua.execute("""
        local view = setmetatable({
            min_month = "2024-12", max_month = "2025-03", cur_month = "2025-02",
            repaints = 0, _populateItems = function(self) self.repaints = self.repaints + 1 end,
        }, {__index = CalendarView})
        view:mountControls()
        view.page_info_first_chev.callback()
        assert(view.cur_month == "2024-12")
        view.page_info_left_chev.callback()
        assert(view.cur_month == "2024-12")
        view.page_info_last_chev.callback()
        assert(view.cur_month == "2025-03")
        view.page_info_right_chev.callback()
        assert(view.cur_month == "2025-03")
        view.browse_future_months = true
        view.page_info_right_chev.callback()
        assert(view.cur_month == "2025-04")
        view.page_info_last_chev.callback()
        assert(view.cur_month == "2025-03")
        view.browse_future_months = false

        local field = view.page_info_text.hold_input
        assert(view.page_info_text.call_hold_input_on_tap and field.input_func() == "2025-03")
        field.callback("2025-00")
        assert(view.cur_month == "2025-03" and UIManager.last.text ~= nil)
        field.callback("9999-12")
        assert(view.cur_month == "9999-12")
        field.callback("1000-01")
        assert(view.cur_month == "1000-01")
        -- Exercise step bounds at a date the host Lua 5.1 calendar can convert.
        field.callback("2022-01")
        view.page_info_left_chev.callback()
        assert(view.cur_month == "2022-01")
        view.page_info_right_chev.callback()
        assert(view.cur_month == "2022-02")

        field.callback("2025-01")
        local close = view:dayReturn()
        close({day_ts = os.time{year = 2024, month = 12, day = 31, hour = 0}})
        assert(view.cur_month == "2025-01" and UIManager.queued ~= nil)
        UIManager.queued()
        assert(view.cur_month == "2024-12")
    """)
    print(f"KOReader {PIN} Lua 5.1: month edges, exact input and cross-month timeline return passed")


if __name__ == "__main__":
    main()
