#!/usr/bin/env python3
"""Run pinned KOReader Book Map and Page Browser tap routing under Lua 5.1."""
import argparse
import hashlib
from pathlib import Path

from lupa.lua51 import LuaRuntime

PIN = "9192014d8bd82a91dc1012473be0f238dedfdb54"


def segment(source, begin, end):
    start = source.index(begin)
    return source[start:source.index(end, start + len(begin))]


def exact(path, sha256):
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != sha256:
        raise SystemExit(f"{path.name} must be from the unmodified KOReader {PIN} checkout")
    return raw.decode()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("widgets", type=Path, help="the pinned frontend/ui/widget directory")
    args = parser.parse_args()
    book_map = exact(
        args.widgets / "bookmapwidget.lua",
        "c734c9b928377a42026bb3dd44153e604bb630bd65ea8f607c9776ef02b6dcbc",
    )
    browser = exact(
        args.widgets / "pagebrowserwidget.lua",
        "346e5d56fee159920405e57999a47185fe111e759b9cdff23e6eb59855f0d9cd",
    )
    option = segment(
        book_map,
        '        {{\n            text = Device:isTouchDevice() and _("Page browser on tap")',
        '        {{\n            text = _("Alternative theme")',
    )
    lua = LuaRuntime()
    lua.execute("""
        BookMapWidget, PageBrowserWidget = {}, {}
        Device = { isTouchDevice = function() return true end }
        BD = {mirroredUILayout = function() return false end}
        _ = function(text) return text end
        logger = {dbg = function() end}
        Event = {new = function(_, ...) return {...} end}
        G_reader_settings = {
            nilOrTrue = function(self, name) return self[name] ~= false end,
            isFalse = function(self, name) return self[name] == false end,
            flipNilOrTrue = function(self, name) self[name] = self[name] == false end,
        }
        UIManager = {
            show = function(self, widget) self.last = widget end,
            close = function(self, widget) self.closed = widget end,
            setDirty = function(self, widget) self.refreshed = widget end,
        }
        function PageBrowserWidget:new(widget)
            return setmetatable(widget, {__index = self})
        end
        package.preload["ui/widget/pagebrowserwidget"] = function() return PageBrowserWidget end
    """)
    lua.execute("function BookMapWidget:option() return ({" + option + "})[1][1] end")
    lua.execute(segment(book_map, "function BookMapWidget:onTap(", "function BookMapWidget:onGoToFocusedPage("))
    lua.execute(segment(browser, "function PageBrowserWidget:onClose(", "function PageBrowserWidget:saveSettings("))
    lua.execute("""
        local entry = {toc_item_page = 23}
        local map = setmetatable({
            cropped = true,
            cropping_widget = {dimen = {}},
            title_bar_h = 80,
            getVGroupRowAtY = function(self, y) assert(y == 50); return entry end,
            onClose = function(self, all) self.closed = all end,
            ui = {
                link = {addCurrentLocationToStack = function(self) self.pushed = (self.pushed or 0) + 1 end},
                handleEvent = function(self, event) self.jump = event end,
            },
        }, {__index = BookMapWidget})
        local tap = {pos = {x = 120, y = 130, notIntersectWith = function() return false end}}
        local option = map:option()
        assert(option.checked_func())
        assert(map:onTap(nil, tap))
        local overview = UIManager.last
        assert(overview.launcher == map and overview.focus_page == 23)
        assert(map.ui.link.pushed == nil and map.ui.jump == nil)
        assert(overview:onClose(false))
        assert(UIManager.closed == overview and UIManager.refreshed == map and map.closed == nil)

        entry = {start_page = 10, getPageAtX = function(_, x, at_bounds)
            assert(x == 120 and at_bounds)
            return 16
        end}
        assert(map:onTap(nil, tap) and UIManager.last.focus_page == 16)
        option.callback()
        assert(not option.checked_func())
        assert(map:onTap(nil, tap))
        assert(map.closed and map.ui.link.pushed == 1)
        assert(map.ui.jump[1] == "GotoPage" and map.ui.jump[2] == 16)
        option.callback()
        assert(option.checked_func())
    """)
    print(f"KOReader {PIN} Lua 5.1: Book Map chapter/page focus, parent return, and direct tap passed")


if __name__ == "__main__":
    main()
