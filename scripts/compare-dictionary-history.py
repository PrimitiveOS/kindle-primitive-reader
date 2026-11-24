#!/usr/bin/env python3
"""Run pinned KOReader dictionary history callbacks under Lua 5.1."""
import argparse
import hashlib
from pathlib import Path

from lupa.lua51 import LuaRuntime

PIN = "9192014d8bd82a91dc1012473be0f238dedfdb54"
SHA256 = "8ca93c7881b7bca33d9afb076e32e85bf203543d80a0b1e053910667b6df79f9"


def section(source, start, stop):
    begin = source.index(start)
    return source[begin:source.index(stop, begin + len(start))]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("frontend", type=Path, help="pinned KOReader checkout frontend directory")
    frontend = parser.parse_args().frontend
    data = (frontend / "apps/reader/modules/readerdictionary.lua").read_bytes()
    if hashlib.sha256(data).hexdigest() != SHA256:
        parser.error(f"Use the unmodified dictionary module from KOReader {PIN}")
    source = data.decode("utf-8")

    # Cut before the hardware or provider dependency. The recorded event and
    # data write above it run verbatim.
    recorder = section(
        source,
        "function ReaderDictionary:stardictLookup(",
        "    if Device:canExternalDictLookup()",
    )
    menu_items = section(
        source,
        "    menu_items.dictionary_lookup_history = ",
        "    menu_items.dictionary_settings = ",
    )
    privacy = section(
        source,
        '            {\n                text = _("Dictionary lookup history"),',
        "            { -- setting used by dictquicklookup",
    )

    lua = LuaRuntime()
    lua.execute("""
        _ = function(text) return text end
        ReaderDictionary = {}
        Event = {new = function(_, ...) return {...} end}
        local records = {}
        lookup_history = {
            addTableItem = function(_, key, item)
                assert(key == "lookup_history")
                table.insert(records, item)
            end,
            readSetting = function(_, key) if key == "lookup_history" then return records end end,
            has = function(_, key) return key == "lookup_history" and #records > 0 end,
            reset = function(_, _) records = {} end,
        }
        UIManager = {show = function(self, widget) self.last = widget end}
        KeyValuePage = {new = function(_, widget) return widget end}
        ConfirmBox = {new = function(_, widget) return widget end}
        G_reader_settings = {
            saveSetting = function(self, key, value) self[key] = value end,
        }
    """)
    lua.execute(recorder + "\nend")
    lua.execute("function ReaderDictionary:menu(menu_items)\n" + menu_items + "\nend")
    lua.execute("function ReaderDictionary:privacy() return {\n" + privacy + "} end")
    lua.execute("""
        local dictionary = setmetatable({
            ui = {handleEvent = function(self, ev) self.last_event = ev end},
        }, {__index = ReaderDictionary})
        local time = 1700000000
        os.time = function() return time end
        local function add(title, word)
            dictionary.ui.doc_props = title and {display_title = title} or nil
            dictionary:stardictLookup(word)
            assert(dictionary.ui.last_event[1] == "WordLookedUp")
            assert(dictionary.ui.last_event[2] == word)
            assert(dictionary.ui.last_event[3] == (title or "Dictionary lookup"))
            time = time + 60
        end
        add("Field Notes", "tide")
        add("Field Notes", "tide")
        add("Coastal map", "ferry")
        add("Field Notes", "quay")
        add(nil, "dock")
        local menu = {}
        dictionary:menu(menu)
        local history = menu.dictionary_lookup_history
        assert(history.enabled_func())
        history.callback()
        local rows = UIManager.last.kv_pairs
        assert(#rows == 9)
        assert(rows[1][1] == "Dictionary lookup:" and rows[2][2] == "dock")
        assert(rows[3][1] == "Field Notes:" and rows[4][2] == "quay")
        assert(rows[5][1] == "Coastal map:" and rows[6][2] == "ferry")
        assert(rows[7][1] == "Field Notes:" and rows[8][2] == "tide" and rows[9][2] == "tide")
        assert(rows[2][1] == os.date("%Y-%m-%d %H:%M:%S", 1700000240))
        assert(rows[9][1] == os.date("%Y-%m-%d %H:%M:%S", 1700000000))

        function dictionary:onLookupWord(word, sane) self.repeated = sane and word end
        rows[6].callback()
        assert(dictionary.repeated == "ferry")
        local privacy = dictionary:privacy()[1].sub_item_table
        assert(privacy[1].checked_func())
        privacy[1].callback()
        assert(not privacy[1].checked_func() and G_reader_settings.disable_lookup_history)
        add("Coastal map", "no log")
        assert(#lookup_history:readSetting("lookup_history") == 5)
        local updated = false
        privacy[2].callback({updateItems = function() updated = true end})
        assert(history.enabled_func())
        UIManager.last.ok_callback()
        assert(updated and not history.enabled_func())
    """)
    print(f"KOReader {PIN} Lua 5.1: repeat logging, grouping, local dates, repeat callback and privacy passed")


if __name__ == "__main__":
    main()
