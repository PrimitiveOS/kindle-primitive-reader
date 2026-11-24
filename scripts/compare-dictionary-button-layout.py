#!/usr/bin/env python3
"""Replay pinned KOReader dictionary button editing and layout."""
import argparse
import hashlib
from pathlib import Path

from lupa.lua51 import LuaRuntime

PIN = "9192014d8bd82a91dc1012473be0f238dedfdb54"
SOURCES = (
    ("apps/reader/modules/readerdictionary.lua", "8ca93c7881b7bca33d9afb076e32e85bf203543d80a0b1e053910667b6df79f9"),
    ("ui/widget/dictquicklookup.lua", "1097fe3fbf4ef26c4c1b259d044af72f8258523537d73ad843ec5ce3a7c281bd"),
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
        raw = (root / "frontend" / name).read_bytes()
        if hashlib.sha256(raw).hexdigest() != expected:
            parser.error(f"Use the unmodified {name} from KOReader {PIN}")
        pinned.append(raw.decode())
    dictionary, popup = pinned

    lua = LuaRuntime()
    lua.execute("""
        ReaderDictionary = {}
        DictQuickLookup = {}
        _ = function(text) return text end
        T = function(text, first, second)
            return (text:gsub("%%1", tostring(first)):gsub("%%2", tostring(second)))
        end
        local function copy(value)
            if type(value) ~= "table" then return value end
            local out = {}
            for key, field in pairs(value) do out[key] = copy(field) end
            return out
        end
        util = { tableDeepCopy = copy }
        ffiUtil = { orderedPairs = pairs }
        Device = { hasDPad = function() return false end }
        Size = { border = { window = 0 }, padding = { default = 0 } }
        G_reader_settings = {
            readSetting = function(self, key) return self[key] end,
            saveSetting = function(self, key, value) self[key] = copy(value) end,
            delSetting = function(self, key) self[key] = nil end,
            hasNot = function(self, key) return self[key] == nil end,
        }
        local function widget(_, settings) return settings end
        SortWidget = { new = widget }
        ConfirmBox = { new = widget }
        require = function(name)
            assert(name == "ui/widget/spinwidget")
            return { new = widget }
        end
        UIManager = { show = function(_, settings) last_widget = settings end }
    """)
    lua.execute(segment(dictionary, "function ReaderDictionary:_genCustomizeButtonsMenu()", "\nfunction ReaderDictionary:showPreferredDictsDialog"))
    lua.execute(segment(popup, "function DictQuickLookup:buildButtonLayout()", "\nfunction DictQuickLookup:"))
    lua.execute("""
        local controller = setmetatable({
            _dict_buttons = {},
            default_layout = {{"prev_dict", "highlight", "next_dict"}, {"wikipedia", "search", "close"}},
            showDict = function(_, word, result)
                assert(word == "lorem")
                assert(result[1].dict == "Layout preview")
                last_preview = result[1].definition
            end,
        }, {__index = ReaderDictionary})
        local menu = controller:_genCustomizeButtonsMenu()
        assert(menu[1].text == "Sort and toggle buttons")
        assert(menu[2].text_func() == "Max buttons in row 1: 3")
        menu[1].callback()
        local sorter = last_widget
        local translate
        for i, item in ipairs(sorter.item_table) do
            if item.id == "wikipedia" then assert(item.checked_func()); item.callback() end
            if item.id == "translate" then
                assert(not item.checked_func()); item.callback(); translate = i
            end
        end
        table.insert(sorter.item_table, 1, table.remove(sorter.item_table, translate))
        assert(G_reader_settings.dict_button_config == nil)
        sorter.callback()
        local function config() return G_reader_settings.dict_button_config end
        assert(table.concat(config().layout[1], ",") == "translate,prev_dict,highlight")
        menu[2].callback()
        assert(last_widget.value_min == 1 and last_widget.value_max == 4)
        last_widget.value = 4
        last_widget.callback(last_widget)
        assert(table.concat(config().layout[1], ",") == "translate,prev_dict,highlight,next_dict")
        menu[3].callback()
        last_widget.value = 1
        last_widget.callback(last_widget)
        assert(#config().layout == 3 and config().layout[3][1] == "close")
        menu[#menu].callback()
        assert(last_preview:find("mock definition", 1, true))

        local view = setmetatable({
            ui = {dictionary = controller},
            width = 1000,
            populatePluginButtons = function() end,
            _getButtonPool = function()
                local pool = {}
                for _, id in ipairs(config().order) do pool[id] = {id = id} end
                pool.prev_dict.can_shrink = true
                pool.prev_dict.pairs_with = "next_dict"
                pool.next_dict.can_shrink = true
                pool.next_dict.pairs_with = "prev_dict"
                return pool
            end,
        }, {__index = DictQuickLookup})
        local layout = view:buildButtonLayout()
        assert(layout[1][1].id == "translate")
        assert(layout[1][2].width == 150 and layout[1][4].width == 150)
        assert(layout[1][3].width == nil and layout[3][1].id == "close")
        menu[1].hold_callback()
        assert(last_widget.ok_text == "Reset" and config() ~= nil)
        last_widget.ok_callback()
        assert(config() == nil and menu[2].text_func() == "Max buttons in row 1: 3")
    """)
    print(f"KOReader {PIN} on {lua.lua_implementation}: sort, toggle, 1–4 per row, paired arrows, preview, and reset")


if __name__ == "__main__":
    main()
