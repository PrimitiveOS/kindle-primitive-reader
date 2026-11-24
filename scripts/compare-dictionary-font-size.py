#!/usr/bin/env python3
"""Replay the pinned dictionary settings callback and number picker in Lua."""
import argparse
import hashlib
import re
from pathlib import Path

from lupa.lua51 import LuaRuntime

PIN = "9192014d8bd82a91dc1012473be0f238dedfdb54"
FILES = {
    "apps/reader/modules/readerdictionary.lua": "8ca93c7881b7bca33d9afb076e32e85bf203543d80a0b1e053910667b6df79f9",
    "ui/widget/dictquicklookup.lua": "1097fe3fbf4ef26c4c1b259d044af72f8258523537d73ad843ec5ce3a7c281bd",
    "ui/widget/spinwidget.lua": "1c15758546b4cb2b49e5f1025f33ef3e83a0687c905b6806e25cbc6f0ce6d2c3",
    "ui/widget/numberpickerwidget.lua": "835225f9c13a1d12d2a830d815ce28b598aa7d6499e445140c3e01e95b197f2f",
}


def pinned(root, name):
    raw = (root / "frontend" / name).read_bytes()
    if hashlib.sha256(raw).hexdigest() != FILES[name]:
        raise SystemExit(f"Use the unmodified {name} from KOReader {PIN}")
    return raw.decode("utf-8")


def segment(source, start, end):
    first = source.index(start)
    return source[first:source.index(end, first + len(start))]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path, help="pinned KOReader source tree")
    args = parser.parse_args()
    menu, popup, spin, number = (pinned(args.source, name) for name in FILES)

    lua = LuaRuntime()
    lua.execute("""
        _ = function(text) return text end
        T = function(text, value) return (text:gsub("%%1", tostring(value))) end
        G_reader_settings = {
            readSetting = function(self, key) return self[key] end,
            saveSetting = function(self, key, value) self[key] = value end,
        }
        Font = { getFace = function(_, family, size)
            assert(family == "cfont")
            return size
        end }
        SpinWidget = {}
        NumberPickerWidget = {}
        local loaded
        SpinWidget.new = function(_, spec) loaded = spec; return spec end
        require = function(name)
            assert(name == "ui/widget/spinwidget")
            return SpinWidget
        end
        UIManager = { show = function(_, settings) assert(settings == loaded); active = settings end }
    """)
    setting = segment(
        menu,
        '{ -- setting used by dictquicklookup\n                text_func = function()\n                    local font_size',
        "\n        }",
    ).rstrip().removesuffix(",")
    lua.execute("setting = " + setting)
    lua.execute(
        "function create_popup(self)\n"
        + segment(popup, "    self.dict_font_size = ", "    self.allow_key_text_selection")
        + "end"
    )
    for setting_name in ("value_step", "value_hold_step", "wrap"):
        value = re.search(r"^    " + setting_name + r" = (\w+),", spin, re.M)[1]
        lua.execute(f"SpinWidget.{setting_name} = {value}")
    lua.execute(segment(spin, "function SpinWidget:onSpinButtonPressed", "\nreturn SpinWidget"))
    lua.execute(segment(number, "function NumberPickerWidget:changeValue", "\n--[[--"))
    lua.execute("""
        assert(setting.text_func() == "Font size: 20")
        local updated = 0
        setting.callback({updateItems = function() updated = updated + 1 end})
        local spin = setmetatable(active, {__index = SpinWidget})
        assert(spin.value_min == 8 and spin.value_max == 32 and spin.default_value == 20)
        spin.value_widget = setmetatable({
            value = spin.value,
            value_min = spin.value_min,
            value_max = spin.value_max,
            wrap = spin.wrap,
            update = function() end,
        }, {__index = NumberPickerWidget})
        spin:onSpinButtonPressed({1, false})
        assert(spin.value_widget.value == 21)
        spin:onSpinButtonPressed({1, true})
        assert(spin.value_widget.value == 25)
        assert(setting.text_func() == "Font size: 20")
        for i = 1, 10 do spin:onSpinButtonPressed({1, true}) end
        assert(spin.value_widget.value == 32)
        spin.value = spin.value_widget.value
        spin:callback()
        assert(updated == 1 and setting.text_func() == "Font size: 32")
        local viewport = {}
        create_popup(viewport)
        assert(viewport.content_face == 32)
        for i = 1, 10 do spin:onSpinButtonPressed({-1, true}) end
        assert(spin.value_widget.value == 8)
        spin.value = spin.value_widget.value
        spin:callback()
        create_popup(viewport)
        assert(viewport.content_face == 8 and viewport.image_alt_face == 8)
    """)
    print(f"KOReader {PIN} on {lua.lua_implementation}: size 8–32, default 20, tap ±1, hold ±4 and popup font after Apply")


if __name__ == "__main__":
    main()
