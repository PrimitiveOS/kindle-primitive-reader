#!/usr/bin/env python3
"""Replay the pinned continuous reader menus and pan handlers."""
import argparse
import hashlib
from pathlib import Path

from lupa.lua51 import LuaRuntime

PIN = "9192014d8bd82a91dc1012473be0f238dedfdb54"
SOURCES = {
    "readerscrolling": "e76cb7f1da4bcc14675756a197fdd0ee953817bf99f54dbb0dd6f3b91da14c8d",
    "readerrolling": "4af978e7d81fa91d8f3a93096b9c656edf0a7c47078a305ea35ac87d9d72d68c",
    "readerpaging": "d110f266d90ceb48699c85118f74c0c8d213e50dd2d6c2301a7c499abc65fcd7",
}


def segment(source, start, end):
    first = source.index(start)
    return source[first:source.index(end, first + len(start))]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("frontend", type=Path, help="pinned KOReader frontend directory")
    root = parser.parse_args().frontend / "apps/reader/modules"
    sources = {}
    for name, expected in SOURCES.items():
        raw = (root / f"{name}.lua").read_bytes()
        if hashlib.sha256(raw).hexdigest() != expected:
            parser.error(f"Use the unmodified {name}.lua from KOReader {PIN}")
        sources[name] = raw.decode()

    lua = LuaRuntime()
    lua.execute("""
        _ = function(value) return value end
        C_ = function(_, value) return value end
        T = function(format, value) return (format:gsub("%%1", tostring(value))) end
        ReaderScrolling = {
            SCROLL_METHOD_CLASSIC = "classic", SCROLL_METHOD_TURBO = "turbo",
            SCROLL_METHOD_ON_RELEASE = "on_release",
            setupTouchZones = function() end,
            cancelInertialScroll = function() return false end,
            cancelledByTouch = function() return false end,
            accountManualScroll = function() end,
            startInertialScroll = function() return false end,
        }
        ReaderRolling, ReaderPaging = {}, {}
        BD = { flipDirectionIfMirroredUILayout = function(dir) return dir end }
        G_reader_settings = {
            readSetting = function(self, key) return self[key] end,
            saveSetting = function(self, key, value) self[key] = value end,
            delSetting = function(self, key) self[key] = nil end,
        }
        UIManager = {
            show = function(_, widget) LastWidget = widget end,
            setDirty = function() end,
        }
        Screen = { getWidth = function() return 1264 end }
        Event = { new = function(_, ...) return {...} end }
        require = function(name)
            assert(name == "ui/widget/spinwidget")
            return { new = function(_, settings) return settings end }
        end
    """)
    lua.execute(segment(
        sources["readerscrolling"],
        "function ReaderScrolling:getDefaultScrollActivationDelay_ms",
        "function ReaderScrolling:onReaderReady",
    ))
    lua.execute(segment(
        sources["readerscrolling"],
        "function ReaderScrolling:applyScrollSettings",
        "function ReaderScrolling:setupTouchZones",
    ))
    lua.execute(segment(sources["readerrolling"], "function ReaderRolling:onSwipe(", "function ReaderRolling:onPosUpdate"))
    lua.execute(segment(sources["readerpaging"], "function ReaderPaging:onSwipe(", "function ReaderPaging:onZoomModeUpdate"))
    lua.execute("""
        local shared = setmetatable({
            view = { view_mode = "page", page_scroll = false },
            scroll_method = "classic",
            scroll_activation_delay_ms = 500,
            ui = { handleEvent = function(_, event) LastSetting = event end },
        }, {__index = ReaderScrolling})
        local menu = {}
        shared:addToMainMenu(menu)
        menu = menu.scrolling
        assert(not menu.enabled_func())
        shared.view.view_mode = "scroll"
        assert(menu.enabled_func())
        shared.view.view_mode = "page"
        shared.view.page_scroll = true
        assert(menu.enabled_func())
        assert(#menu.sub_item_table == 4)
        assert(shared:getDefaultScrollActivationDelay_ms() == 500)
        G_reader_settings:saveSetting("activate_menu", "tap")
        assert(shared:getDefaultScrollActivationDelay_ms() == 0)
        shared.ui.gestures = { multiswipes_enabled = true }
        assert(shared:getDefaultScrollActivationDelay_ms() == 500)
        shared.ui.gestures = nil
        for index, method in ipairs({"classic", "turbo", "on_release"}) do
            local item = menu.sub_item_table[index]
            item.callback()
            assert(item.radio and item.checked_func() and shared.scroll_method == method)
        end
        menu.sub_item_table[4].callback()
        assert(LastWidget.value_min == 0 and LastWidget.value_max == 2000)
        assert(LastWidget.default_value == 0)
        assert(LastWidget.value_step == 100 and LastWidget.value_hold_step == 500)
        LastWidget.value = 450
        LastWidget.callback(LastWidget)
        assert(G_reader_settings:readSetting("scroll_activation_delay") == 450)
        assert(LastSetting[2] == "on_release" and LastSetting[4] == 450)
        LastWidget.value = 0
        LastWidget.callback(LastWidget)
        assert(G_reader_settings:readSetting("scroll_activation_delay") == nil)

        local function fixture(prototype, method, delay)
            return setmetatable({
                view = { view_mode = "scroll", page_scroll = true, page_states = {{page = 0}} },
                current_pos = 0,
                scroll_method = method,
                scroll_activation_delay = delay or 0,
                pan_interval = 200,
                ui = {
                    scrolling = ReaderScrolling,
                    document = { getXPointer = function() return "source" end },
                },
                _gotoPos = function(self, y) self.current_pos = y end,
                _gotoPage = function(self, y) self.current_pos = y end,
                onPanningRel = function(self, dy)
                    self.current_pos = self.current_pos + dy
                    self.view.page_states = {{page = self.current_pos}}
                end,
            }, {__index = prototype})
        end
        local function pan(reader, y, ms)
            reader:onPan(nil, {direction = "north", relative = {y = y}, time = ms})
            return reader.current_pos
        end
        for _, prototype in ipairs({ReaderRolling, ReaderPaging}) do
            for _, case in ipairs({
                {"classic", 80, 220}, {"turbo", 80, 300}, {"on_release", 0, 0},
            }) do
                local reader = fixture(prototype, case[1])
                assert(pan(reader, -80, 1000) == case[2])
                assert(pan(reader, -220, 1220) == case[3])
                reader:onPanRelease()
                local final = case[1] == "on_release" and 220 or case[3]
                assert(reader.current_pos == final)
            end
            local quick = fixture(prototype, "classic", 500)
            assert(pan(quick, -80, 1000) == 0)
            assert(pan(quick, -120, 1299) == 0)
            assert(not quick:onSwipe(nil, {direction = "north", time = 1300}))
            assert(quick.current_pos == 0)

            local stationary = fixture(prototype, "on_release", 500)
            assert(pan(stationary, -80, 1000) == 0)
            stationary:onPanRelease(nil, {time = 1900})
            assert(stationary.current_pos == 0)

            local reader = fixture(prototype, "classic", 500)
            assert(pan(reader, -80, 1000) == 0)
            assert(pan(reader, -120, 1499) == 0)
            assert(pan(reader, -150, 1500) == 150)
            reader:onHandledAsSwipe()
            assert(reader.current_pos == 0)
        end
    """)
    print(f"KOReader {PIN} on {lua.lua_implementation}: continuous gating, modes, delay, release and swipe rollback")


if __name__ == "__main__":
    main()
