#!/usr/bin/env python3
"""Exercise pinned KOReader statistics callbacks, book ordering and week spans."""
import argparse
import hashlib
import os
from pathlib import Path
import sqlite3
import time

from lupa.lua51 import LuaRuntime


PIN = "9192014d8bd82a91dc1012473be0f238dedfdb54"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("checkout", type=Path, help="root of the pinned KOReader source")
checkout = parser.parse_args().checkout


def read(path, sha):
    data = (checkout / path).read_bytes()
    if hashlib.sha256(data).hexdigest() != sha:
        parser.error(f"Use {path} from KOReader {PIN}")
    return data.decode("utf-8")


stats = read("plugins/statistics.koplugin/main.lua", "da6db5e381bcfdaf05053d23bf75781166cc713b14076a65fcd2c527067fd088")
calendar = read("plugins/statistics.koplugin/calendarview.lua", "80e232d13a46660c2c0c9aafb2a043668c36175d57a8ee5b1539c583b00347b1")
picker = read("frontend/ui/widget/numberpickerwidget.lua", "835225f9c13a1d12d2a830d815ce28b598aa7d6499e445140c3e01e95b197f2f")

lua = LuaRuntime(unpack_returned_tuples=True)
lua.execute("""
    _ = function(text) return text end
    T = function(text) return text end
    UIManager = { show = function(_, widget) shown = widget end }
    require = function() return { new = function(_, widget) return widget end } end
    DEFAULT_CALENDAR_NB_BOOK_SPANS = 3
    CalendarWeek = {}
    NumberPickerWidget = {}
""")
start = stats.rfind("\n                    {", 0, stats.index('return T(_("Books per calendar day:'))
end = stats.rfind("\n                    {", 0, stats.index('return T(_("Cloud sync:'))
lua.globals().controls = lua.eval("function(self) return {" + stats[start:end] + "} end")
start = calendar.index("function CalendarWeek:addDay(")
end = calendar.index("\n-- Set of", start)
lua.execute(calendar[start:end])
start = picker.index("function NumberPickerWidget:changeValue(")
end = picker.index("\n--[[", start)
lua.execute(picker[start:end])

lua.execute("""
    local stats = {settings = {calendar_nb_book_spans = 3}}
    local menu = controls(stats)
    local updated = 0
    local context = {updateItems = function() updated = updated + 1 end}
    menu[1].callback(context)
    assert(shown.value == 3 and shown.value_min == 1 and shown.value_max == 5 and shown.default_value == 3)
    shown.callback({value = 2})
    assert(stats.settings.calendar_nb_book_spans == 2 and updated == 1)
    menu[4].callback(context)
    assert(shown.hour == 0 and shown.min == 0 and shown.min_step == 10 and shown.min_hold_step == 30 and shown.min_max == 50)
    shown.callback({hour = 4, min = 20})
    assert(stats.settings.calendar_day_start_hour == 4 and stats.settings.calendar_day_start_minute == 20 and updated == 2)
    assert(not menu[5].checked_func())
    menu[5].callback()
    assert(menu[5].checked_func())
    menu[5].callback()
    assert(not menu[5].checked_func() and stats.settings.calendar_day_start_minute == 20)
    local minute = setmetatable({value = 0, value_min = 0, value_max = 50, wrap = true}, {__index = NumberPickerWidget})
    assert(minute:changeValue(-10) == 50 and minute:changeValue(30) == 30)
    minute.value = 40
    assert(minute:changeValue(30) == 0)

    local week = setmetatable({nb_book_spans = 2, days_books = {}, calday_widgets = {}}, {__index = CalendarWeek})
    local hidden = {}
    for _, identifiers in ipairs({{1, 2, 3}, {3, 2, 1}, {3, 4, 2}, {}, {3, 2}, {2}}) do
        local books = {}
        for index, id in ipairs(identifiers) do books[index] = { id = id } end
        week:addDay({read_books = books, updateNbNotShown = function(_, count) hidden[#hidden + 1] = count end})
    end
    assert(#hidden == 3 and hidden[1] == 1)
    assert(week.days_books[1][2].span_days == 2 and week.days_books[2][2].id == 2)
    assert(week.days_books[2][1].span_days == 2 and week.days_books[3][1].id == 3)
    assert(week.days_books[4][1] == false and week.days_books[5][1].span_days == 1)
    assert(week.days_books[5][2].span_days == 2 and week.days_books[6][2].id == 2)
    local rank_change = setmetatable({nb_book_spans = 3, days_books = {}, calday_widgets = {}}, {__index = CalendarWeek})
    rank_change:addDay({read_books = {{id = 1}, {id = 2}, {id = 3}}})
    rank_change:addDay({read_books = {{id = 4}, {id = 5}, {id = 1}}})
    assert(rank_change.days_books[2][1].id == 1 and rank_change.days_books[2][2].id == 5 and rank_change.days_books[2][3].id == 4)
""")

os.environ["TZ"] = "UTC"
time.tzset()
start = stats.index("function ReaderStatistics:getReadBookByDay(")
query = stats[start:].split("[[", 1)[1].split("]]", 1)[0]

with sqlite3.connect(":memory:") as database:
    database.executescript("""
        CREATE TABLE book (id INTEGER, title TEXT);
        CREATE TABLE page_stat (id_book INTEGER, start_time INTEGER, duration INTEGER);
        INSERT INTO book VALUES (1, 'Harbor notes'), (2, 'Coast atlas'), (3, 'Bird log');
        INSERT INTO page_stat VALUES
            (1, strftime('%s', '2026-11-08 02:30:00'), 50),
            (2, strftime('%s', '2026-11-08 02:40:00'), 60),
            (1, strftime('%s', '2026-11-08 02:50:00'), 10),
            (3, strftime('%s', '2026-11-08 09:00:00'), 80);
    """)
    month = "2026-11-01"
    raw = database.execute(query, (0, month, month)).fetchall()
    shifted = database.execute(query, (260 * 60, month, month)).fetchall()
    assert [(day, duration, book) for day, duration, book, _ in raw] == [
        ("2026-11-08", 80, 3), ("2026-11-08", 60, 1), ("2026-11-08", 60, 2),
    ]
    assert [(day, book) for day, _, book, _ in shifted] == [
        ("2026-11-07", 1), ("2026-11-07", 2), ("2026-11-08", 3),
    ]

print(f"KOReader {PIN}: calendar callbacks, time picker, day shift, duration ranking and continuous book spans passed")
