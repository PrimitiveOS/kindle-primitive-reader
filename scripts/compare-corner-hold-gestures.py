#!/usr/bin/env python3
"""Replay the pinned KOReader corner-hold settings and gesture dispatch."""
import argparse
import hashlib
from pathlib import Path
import re

from lupa.lua51 import LuaRuntime

PIN = "9192014d8bd82a91dc1012473be0f238dedfdb54"
SOURCES = (
    ("plugins/gestures.koplugin/main.lua", "ffc9c41f162bbafe3aaa792888d4bda53be4ec451e9ba4934e696cf00c4e6c9e"),
    ("frontend/ui/elements/common_settings_menu_table.lua", "0f3a9103a47e5067bc48a316bab69f18c0f06997cb7d0625f96273c9efffd171"),
    ("defaults.lua", "be7c335d81dc6d7bebc608e15584b569ac2a541fbbabd019ad53719e3853f78f"),
    ("frontend/dispatcher.lua", "b311c9152317fd78f0992b7483e13878f06ce6eb0b21b029c1bb4938a5301d86"),
)


def segment(source, start, end):
    first = source.index(start)
    return source[first:source.index(end, first + len(start))]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, help="pinned KOReader source tree")
    root = parser.parse_args().source
    pinned = []
    for name, expected in SOURCES:
        raw = (root / name).read_bytes()
        if hashlib.sha256(raw).hexdigest() != expected:
            parser.error(f"Use the unmodified {name} from KOReader {PIN}")
        pinned.append(raw.decode())
    gestures, settings, defaults, dispatcher = pinned
    lua = LuaRuntime()
    lua.execute("""
        _ = function(text) return text end
        C_ = function(_, text) return text end
        local mapped = { NextPage = 1 }
        Gestures = { gestures = { corners = mapped, edges = mapped } }
        common_settings = {}
        G_reader_settings = {
            isTrue = function(self, key) return self[key] == true end,
            readSetting = function(self, key) return self[key] end,
            hasNot = function(self, key) return self[key] == nil end,
            saveSetting = function(self, key, value) self[key] = value end,
            flipNilOrFalse = function(self, key)
                if self[key] then self[key] = nil else self[key] = true end
            end,
        }
        Event = { new = function(_, name, ...) return {name = name, args = {...}} end }
        DispatchCount = 0
        Dispatcher = { execute = function(_, actions, props)
            assert(actions == mapped and props.gesture.ges ~= nil)
            DispatchCount = DispatchCount + 1
        end }
        Gestures.ui = { handleEvent = function(_, event) assert(event.name == "HandledAsSwipe") end }
        UIManager = {
            broadcastEvent = function(_, event)
                Gestures["on" .. event.name](Gestures, unpack(event.args, 1, 2))
            end,
            scheduleIn = function(_, seconds, callback)
                assert(Scheduled == nil)
                Scheduled = {seconds = seconds, callback = callback}
            end,
        }
        require = function(name)
            assert(name == "ui/widget/notification")
            return { notify = function(_, text) Notification = text end }
        end
        Device = { isTouchDevice = function() return true end }
    """)
    lua.execute(segment(gestures, "function Gestures:gestureAction(", "\nfunction Gestures:multiswipeAction"))
    lua.execute(segment(gestures, "function Gestures:onIgnoreHoldCorners(", "\nfunction Gestures:onFlushSettings"))
    lua.execute(segment(settings, "common_settings.ignore_hold_corners =", "\n    common_settings.screen_disable_double_tap"))
    zones = re.findall(r"(DTAP_ZONE_(?:TOP|BOTTOM)_(?:LEFT|RIGHT) = \{[^}]+\}),", defaults)
    assert len(zones) == 4
    lua.execute("\n".join(zones))
    duration = re.search(r"^    ignore_hold_corners = (\{[^}]+\}),$", dispatcher, re.MULTILINE)
    assert duration is not None
    lua.execute("Duration = " + duration[1])
    lua.execute("""
        local function routed(name, event)
            return Gestures:gestureAction(name, {ges = event})
        end
        for _, zone in ipairs({DTAP_ZONE_TOP_LEFT, DTAP_ZONE_TOP_RIGHT,
                              DTAP_ZONE_BOTTOM_LEFT, DTAP_ZONE_BOTTOM_RIGHT}) do
            assert(zone.w == 1/8 and zone.h == 1/8)
        end
        assert(DTAP_ZONE_TOP_RIGHT.x == 7/8 and DTAP_ZONE_BOTTOM_RIGHT.y == 7/8)
        assert(Duration.min == 5 and Duration.max == 30 and Duration.default == 10)
        local menu = common_settings.ignore_hold_corners
        assert(menu.text == "Ignore long-press on corners" and not menu.checked_func())
        assert(routed("corners", "hold") and DispatchCount == 1)
        assert(not routed("unmapped", "hold") and DispatchCount == 1)
        menu.callback()
        assert(menu.checked_func() and Notification == nil)
        assert(not routed("corners", "hold") and DispatchCount == 1)
        assert(routed("corners", "tap") and routed("edges", "swipe") and DispatchCount == 3)
        Gestures:onIgnoreHoldCornersTime(5)
        assert(Scheduled == nil)
        menu.callback()
        assert(not menu.checked_func())
        Gestures:onIgnoreHoldCornersTime(Duration.default)
        assert(Scheduled.seconds == 10 and not routed("corners", "hold"))
        Gestures:onIgnoreHoldCornersTime(30)
        assert(Scheduled.seconds == 10)
        Scheduled.callback()
        assert(not menu.checked_func() and routed("corners", "hold") and DispatchCount == 4)
    """)
    print(f"KOReader {PIN} on {lua.lua_implementation}: corner zones, toggle, timed pause, and fallback dispatch")


if __name__ == "__main__":
    main()
