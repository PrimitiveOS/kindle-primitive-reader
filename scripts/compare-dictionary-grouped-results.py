#!/usr/bin/env python3
"""Exercise the pinned KOReader dictionary result chooser callbacks."""
import argparse
import hashlib
from pathlib import Path

from lupa.lua51 import LuaRuntime

PIN = "9192014d8bd82a91dc1012473be0f238dedfdb54"
SHA256 = "1097fe3fbf4ef26c4c1b259d044af72f8258523537d73ad843ec5ce3a7c281bd"

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("source", type=Path, help="pinned frontend/ui/widget/dictquicklookup.lua")
raw = parser.parse_args().source.read_bytes()
if hashlib.sha256(raw).hexdigest() != SHA256:
    parser.error(f"Use the unmodified dictquicklookup.lua from KOReader {PIN}")
source = raw.decode("utf-8")

lua = LuaRuntime()
lua.execute("""
DictQuickLookup = {}
_ = function(text) return text end
T = function(template, count) return (template:gsub("%%1", tostring(count))) end
util = {arrayContains = function(values, wanted)
    for _, value in ipairs(values) do
        if value == wanted then return true end
    end
    return false
end}
Event = {new = function(_, name, word) return {name = name, word = word} end}
UIManager = {
    opened = {},
    show = function(self, dialog) table.insert(self.opened, dialog) end,
    close = function(self, dialog)
        assert(self.opened[#self.opened] == dialog)
        table.remove(self.opened)
    end,
}
ButtonDialog = {
    new = function(_, config)
        function config:setScrolledOffset(offset) self.offset = offset end
        function config:getScrolledOffset() return self.offset end
        function config:getButtonById(id)
            for _, row in ipairs(self.buttons) do
                for _, button in ipairs(row) do
                    if button.id == id then
                        return {label_widget = {isTruncated = function()
                            return button.text == "intentionally long headword"
                        end}}
                    end
                end
            end
            error("Missing button " .. id)
        end
        return config
    end,
}
""")
start = source.index("function DictQuickLookup:onShowResultsMenu()")
end = source.index("function DictQuickLookup:showWikiResultsMenu()", start)
lua.execute(source[start:end])
start = source.index("        left_icon_tap_callback = function()")
end = source.index("\n    }", start)
lua.globals().callbacks = lua.eval("function(self) return {" + source[start:end] + "} end")

compare = lua.eval("""function()
    local popup = setmetatable({
        width = 1000,
        results = {
            {dict = "Harbour", word = "beacon"},
            {dict = "Field", word = "beacon"},
            {dict = "Harbour", word = "light"},
            {dict = "Island", word = "intentionally long headword"},
        },
        dict_index = 3,
        menu_opened = {},
        menu_scrolled_offsets = {},
        changeDictionary = function(self, index) self.dict_index = index end,
        ui = {handleEvent = function(self, event)
            assert(event.name == "LookupWord")
            self.last_word = event.word
        end},
    }, {__index = DictQuickLookup})
    local function current() return UIManager.opened[#UIManager.opened] end
    local function tap_outside()
        local menu = current()
        menu.tap_close_callback()
        UIManager:close(menu)
        assert(not popup.menu_opened[menu])
    end
    local controls = callbacks(popup)
    controls.left_icon_tap_callback()
    local flat = current()
    assert(#flat.buttons == 4)
    assert(flat.buttons[3][1].font_bold)
    flat.buttons[3][1].hold_callback()
    assert(popup.ui.last_word == "light" and current() == flat)
    flat.offset = 17
    tap_outside()

    controls.left_icon_hold_callback()
    local groups = current()
    assert(#groups.buttons == 3)
    assert(groups.buttons[1][1].text == "Harbour")
    assert(groups.buttons[1][1].font_bold)
    assert(groups.buttons[1][2].text == "2 results")
    assert(groups.buttons[2][2].text == "beacon")
    assert(not groups.buttons[2][1].font_bold)
    groups.buttons[2][2].hold_callback()
    assert(popup.ui.last_word == "beacon" and current() == groups)
    groups.offset = 41

    groups.buttons[1][2].callback()
    local harbour = current()
    assert(#harbour.buttons == 2)
    assert(harbour.buttons[1][1].text == "beacon")
    assert(harbour.buttons[2][1].text == "light" and harbour.buttons[2][1].font_bold)
    harbour.buttons[2][1].hold_callback()
    assert(popup.ui.last_word == "light" and current() == harbour)
    harbour.offset = 69
    tap_outside()
    assert(current() == groups)

    groups.buttons[3][2].callback()
    local island = current()
    assert(island.offset == nil)
    assert(island.buttons[1][1].text == "intentionally long headword")
    island.offset = 9
    tap_outside()
    groups.buttons[1][2].callback()
    harbour = current()
    assert(harbour.offset == 69)
    harbour.buttons[1][1].callback()
    assert(popup.dict_index == 1 and not current())

    controls.left_icon_hold_callback()
    groups = current()
    assert(groups.offset == 41)
    groups.buttons[2][2].callback()
    assert(popup.dict_index == 2 and not current())
    controls.left_icon_hold_callback()
    groups = current()
    groups.buttons[1][1].callback()
    assert(popup.dict_index == 1 and not current())
    controls.left_icon_tap_callback()
    flat = current()
    assert(flat.offset == 17)
    flat.buttons[3][2].callback()
    assert(popup.dict_index == 3 and not current() and not next(popup.menu_opened))
    return true
end""")

assert compare()
print(f"KOReader {PIN} on {lua.lua_implementation}: flat and grouped chooser callbacks, nested headword requests and independent scroll offsets")
