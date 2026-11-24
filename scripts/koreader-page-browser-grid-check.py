#!/usr/bin/env python3
"""Run pinned KOReader Page Browser grid, navigation, and ribbon callbacks."""
import argparse
import hashlib
from pathlib import Path

from lupa.lua51 import LuaRuntime

PIN = "9192014d8bd82a91dc1012473be0f238dedfdb54"
PINNED_FILES = {
    "ui/widget/pagebrowserwidget.lua": "346e5d56fee159920405e57999a47185fe111e759b9cdff23e6eb59855f0d9cd",
    "ui/widget/bookmapwidget.lua": "c734c9b928377a42026bb3dd44153e604bb630bd65ea8f607c9776ef02b6dcbc",
    "device/gesturedetector.lua": "3a2d733a66f94e5cb1cc003c7ba736a03006e7c9242211adc243d74bc2c67db8",
    "ui/bidi.lua": "05ca900fd03f930c39ff6cd4667ffa7f397dcd4a13e09eb8a3884111725853b2",
}


def segment(source, begin, end):
    first = source.index(begin)
    return source[first:source.index(end, first + len(begin))]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("browser", type=Path, help="pinned frontend/ui/widget/pagebrowserwidget.lua")
    args = parser.parse_args()
    def pinned(name):
        path = args.browser if name == "ui/widget/pagebrowserwidget.lua" else args.browser.parents[2] / name
        data = path.read_bytes()
        if hashlib.sha256(data).hexdigest() != PINNED_FILES[name]:
            parser.error(f"Use the unmodified {name} from KOReader {PIN}")
        return data.decode()

    source = pinned("ui/widget/pagebrowserwidget.lua")
    book_map = pinned("ui/widget/bookmapwidget.lua")
    detector = pinned("device/gesturedetector.lua")
    bidi = pinned("ui/bidi.lua")
    menu = segment(
        source,
        '        {\n            {\n                text = _("Thumbnail columns"),',
        '        {\n            {\n                text = _("Thumbnail page numbers"),',
    )
    defaults = segment(source, "    self.nb_rows = self.ui.doc_settings:readSetting", "    self.nb_grid_items =")
    lua = LuaRuntime()
    lua.execute("""
        PageBrowserWidget = {}
        BookMapWidget = {}
        Contact = {}
        Bidi = {_mirrored_ui_layout = false}
        BD = Bidi
        Geom = {new = function(_, geometry) return geometry end}
        logger = {dbg = function() end}
        Screen = {
            w = 1264, h = 1680,
            getWidth = function(self) return self.w end,
            getHeight = function(self) return self.h end,
        }
        UIManager = {setDirty = function(self, _, mode) self.dirty = mode end}
        _ = function(text) return text end
        G_reader_settings = {
            saveSetting = function(self, key, value) self[key] = value end,
            readSetting = function(self, key) return self[key] end,
        }
    """)
    lua.execute("function PageBrowserWidget:controls() return {" + menu + "} end")
    lua.execute("function PageBrowserWidget:loadGrid()\n" + defaults + "\nend")
    lua.execute(segment(source, "function PageBrowserWidget:saveSettings(", "function PageBrowserWidget:updateNbTocSpans("))
    lua.execute(segment(source, "function PageBrowserWidget:updateNbCols(", "function PageBrowserWidget:updateThumbnailPageNumsDisplayType("))
    lua.execute(segment(source, "function PageBrowserWidget:onSwipe(", "function PageBrowserWidget:onPan("))
    lua.execute(segment(source, "function PageBrowserWidget:onTap(", "function PageBrowserWidget:onThumbnailHold("))
    lua.execute(segment(book_map, "function BookMapWidget:onClose(", "function BookMapWidget:getMatchingVGroupRow("))
    lua.execute(segment(source, "function PageBrowserWidget:onScrollPageUp(", "function PageBrowserWidget:onScrollRowUp("))
    lua.execute(segment(detector, "function Contact:getPath(", "function Contact:isSwipe("))
    lua.execute(segment(detector, "function Contact:handleSwipe(", "--[[--\nEmits the pan gestures"))
    lua.execute(segment(bidi, "local mirrored_directions =", "-- Wrap provided text with bidirectionality control characters"))
    lua.execute("""
        local function browser()
            return setmetatable({
                min_nb_cols = 1, min_nb_rows = 1, max_nb_cols = 6, max_nb_rows = 6,
                ui = {
                    handmade = {isHandmadeTocEnabled = function() return false end},
                    doc_settings = {
                        saveSetting = G_reader_settings.saveSetting,
                        readSetting = G_reader_settings.readSetting,
                    },
                },
                relayouts = 0, row_height = 190, pages_per_row = 60,
                updateLayout = function(self) self.relayouts = self.relayouts + 1 end,
                updateFocusPage = function(self, delta, relative)
                    assert(relative)
                    self.delta = delta
                    return true
                end,
                update = function() end,
            }, {__index = PageBrowserWidget})
        end
        local first = browser()
        first:loadGrid()
        assert(first.nb_cols == 3 and first.nb_rows == 2)
        local buttons = first:controls()
        buttons[1][3].callback()
        buttons[2][3].hold_callback()
        assert(first.nb_cols == 4 and first.nb_rows == 4)
        buttons[1][3].hold_callback()
        assert(first.nb_cols == 6 and not buttons[1][3].enabled_func())
        buttons[2][2].hold_callback()
        assert(first.nb_rows == 2)
        assert(first.relayouts == 4)
        assert(first.ui.doc_settings.page_browser_nb_cols == 6)
        assert(G_reader_settings.page_browser_nb_rows == 2)

        local second = browser()
        second:loadGrid()
        assert(second.nb_cols == 6 and second.nb_rows == 2)
        local second_buttons = second:controls()
        second_buttons[1][2].hold_callback()
        second_buttons[2][2].hold_callback()
        assert(second.nb_cols == 4 and second.nb_rows == 1)
        assert(not second_buttons[2][2].enabled_func())
        first:loadGrid()
        assert(first.nb_cols == 6 and first.nb_rows == 2)
        local third = browser()
        third:loadGrid()
        assert(third.nb_cols == 4 and third.nb_rows == 1)

        local function swipe(widget, x, y, dx, dy)
            local contact = setmetatable({
                initial_tev = {x = x, y = y},
                current_tev = {x = x + dx, y = y + dy},
                multiswipe_directions = {},
                ges_dec = {dropContact = function() end},
            }, {__index = Contact})
            local gesture = contact:handleSwipe()
            assert(gesture.pos.x == x and gesture.pos.y == y)
            assert(gesture.end_pos.x == x + dx and gesture.end_pos.y == y + dy)
            widget.delta = nil
            widget.nb_grid_items = widget.nb_cols * widget.nb_rows
            return widget:onSwipe(nil, gesture)
        end

        third.nb_cols, third.nb_rows = 2, 2
        assert(swipe(third, 950, 209, -230, 28) and third.nb_cols == 3)
        assert(swipe(third, 920, 210, -230, 28) and third.delta == 6)
        assert(swipe(third, 157, 660, 65, -240) and third.nb_rows == 3)
        assert(swipe(third, 158, 660, -40, -240) and third.delta == 3)
        assert(swipe(third, 900, 1600, -230, 0) and third.delta == 60)
        assert(not swipe(third, 900, 80, -220, 180) and UIManager.dirty == "full")

        Screen.w, Screen.h = 1680, 1264
        assert(swipe(third, 660, 157, 230, 26) and third.nb_cols == 2)
        assert(swipe(third, 660, 158, 230, 26) and third.delta == -6)
        assert(swipe(third, 209, 660, 65, -240) and third.nb_rows == 4)
        assert(swipe(third, 210, 660, -40, -240) and third.delta == 2)
        Bidi._mirrored_ui_layout = true
        assert(swipe(third, 660, 100, 230, 26) and third.nb_cols == 3)
        assert(swipe(third, 209, 660, 65, -240) and third.nb_rows == 5)
        third.nb_rows = 1
        local relayouts = third.relayouts
        assert(swipe(third, 209, 660, 0, 240) and third.nb_rows == 1)
        assert(third.relayouts == relayouts)

        local taps = browser()
        taps.title_bar_h, taps.nb_grid_items, taps.focus_page_shift = 180, 2, 0
        taps.grid = {
            {dimen = {x = 80, y = 220, w = 500, h = 500}, page_idx = 6, {{}}},
            {dimen = {x = 900, y = 220, w = 500, h = 500}},
        }
        taps.onClose = function(self, jump)
            assert(jump)
            self.action = "jump"
        end
        taps.row = {{getPageAtX = function(_, x) if x == 600 then return 15 end end}}
        taps.updateFocusPage = function(self, page, relative)
            self.action = relative and (page < 0 and "previous" or "next") or page
            if relative then assert(math.abs(page) == self.nb_grid_items) end
            return true
        end
        taps.ui.link = {addCurrentLocationToStack = function() taps.history = true end}
        taps.ui.handleEvent = function(_, event)
            assert(event.handler == "GotoPage" and event.page == 6)
        end
        Event = {new = function(_, handler, page) return {handler = handler, page = page} end}
        local function gesture_at(x, y)
            return {pos = {x = x, y = y, intersectWith = function(self, rect)
                return self.x >= rect.x and self.x < rect.x + rect.w and
                       self.y >= rect.y and self.y < rect.y + rect.h
            end}}
        end
        local function tap(x, y)
            taps.action = nil
            assert(taps:onTap(nil, gesture_at(x, y)))
            return taps.action
        end
        Bidi._mirrored_ui_layout = false
        assert(tap(30, 150) == nil)            -- title occurs before fallback
        assert(tap(30, 220) == "previous")
        assert(tap(840, 220) == "next")      -- exact screen midpoint
        assert(tap(1010, 300) == "next")     -- an empty grid slot
        assert(tap(600, 1200) == 14)           -- ribbon precedes fallback
        assert(tap(1000, 1200) == nil)        -- ribbon with no mapped page
        assert(tap(110, 300) == "jump" and taps.history)
        Bidi._mirrored_ui_layout = true
        assert(tap(30, 220) == "next")

        function BookMapWidget:new(options)
            return setmetatable(options, {__index = self})
        end
        UIManager.show = function(self, widget) self.shown = widget end
        UIManager.close = function(self, widget) self.closed = widget end
        UIManager.setDirty = function(self, widget, mode) self.dirty = {widget, mode} end
        taps.focus_page = 7
        taps.history = false
        taps.onThumbnailHold = function(self, page) self.bookmarked = page end
        assert(taps:onHold(nil, gesture_at(110, 300)) and taps.bookmarked == 6)
        assert(taps:onHold(nil, gesture_at(1000, 1200)) and UIManager.shown == nil)
        assert(taps:onHold(nil, gesture_at(600, 1200)))
        local map = assert(UIManager.shown)
        assert(map.launcher == taps and map.ui == taps.ui and map.focus_page == 15)
        assert(map.extra_symbols_pages[7] == 0x25A2)
        assert(taps.focus_page == 7 and not taps.history)
        taps.action = nil
        assert(map:onClose(false) and UIManager.closed == map and taps.action == nil)
        assert(UIManager.dirty[1] == taps and UIManager.dirty[2] == "ui")
        assert(map:onClose(true) and taps.action == "jump")
    """)
    print(f"KOReader {PIN} Lua 5.1: grid controls, edge swipes, body taps, and ribbon map passed")


if __name__ == "__main__":
    main()
