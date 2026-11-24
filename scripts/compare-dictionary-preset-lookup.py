#!/usr/bin/env python3
"""Execute pinned KOReader's one-query preset callbacks in Lua 5.1."""
import argparse
import hashlib
import re
from pathlib import Path

from lupa.lua51 import LuaRuntime


REFERENCE = "9192014d8bd82a91dc1012473be0f238dedfdb54"


def verified(path, expected):
    data = path.read_bytes()
    if hashlib.sha256(data).hexdigest() != expected:
        raise SystemExit(f"{path} does not match KOReader {REFERENCE}")
    return data.decode()


def method(source, name):
    match = re.search(rf"^function {re.escape(name)}\(.*?(?=^function |\Z)", source, re.M | re.S)
    if not match:
        raise SystemExit(f"Cannot find pinned callback {name}")
    return match.group()


def main():
    args = argparse.ArgumentParser(description=__doc__)
    args.add_argument("frontend", type=Path, help="pinned KOReader checkout frontend directory")
    frontend = args.parse_args().frontend
    dictionary = verified(
        frontend / "apps/reader/modules/readerdictionary.lua",
        "8ca93c7881b7bca33d9afb076e32e85bf203543d80a0b1e053910667b6df79f9",
    )
    quick_lookup = verified(
        frontend / "ui/widget/dictquicklookup.lua",
        "1097fe3fbf4ef26c4c1b259d044af72f8258523537d73ad843ec5ce3a7c281bd",
    )

    lua = LuaRuntime(unpack_returned_tuples=True)
    lua.execute("""
        ReaderDictionary, DictQuickLookup = {}, {}
        _ = function(text) return text end
        UIManager = {
            show = function(self, dialog) self.last = dialog end,
            close = function(self, dialog) if dialog then dialog.closed = true end end,
        }
        InputDialog = { new = function(self, dialog)
            dialog.getInputText = function(widget) return widget.input end
            dialog.onShowKeyboard = function(widget) widget.keyboard = true end
            dialog.onCloseKeyboard = function(widget) widget.keyboard = false end
            return dialog
        end }
        ButtonDialog = { new = function(self, dialog) return dialog end }
        Event = { new = function(self, ...) return {...} end }
        Presets = { getPresets = function(object) return object.names end }
    """)
    for name in ("onShowDictionaryLookup", "showSearchWithPresetDialog", "showNoResultsDialog"):
        lua.execute(method(dictionary, f"ReaderDictionary:{name}"))
    lua.execute(method(quick_lookup, "DictQuickLookup:lookupInputWord"))
    lua.execute("""
        local dictionary = setmetatable({
            preset_obj = { names = {"Coast", "Harbour"}, presets = {
                Coast = {tag = "coast"}, Harbour = {tag = "harbour"},
            } },
            tag = "original",
            highlight = "mark",
        }, { __index = ReaderDictionary })
        local calls = {}
        function dictionary:buildPreset() return {tag = self.tag} end
        function dictionary:loadPreset(preset, quiet)
            assert(quiet)
            self.tag = preset.tag
        end
        function dictionary:showLookupInfo() end
        function dictionary:dismissLookupInfo() end
        function dictionary:onLookupWord(word, sane, boxes, highlight, link, close)
            table.insert(calls, {word, sane, boxes, highlight, link, close})
        end

        assert(dictionary:onShowDictionaryLookup())
        local input = UIManager.last
        input.input = "   "
        input.buttons[1][1].callback()
        assert(UIManager.last == input and dictionary.tag == "original")
        input.input = "beacon"
        input.buttons[1][1].callback()
        local picker = UIManager.last
        assert(not input.keyboard and not input.closed and dictionary.tag == "original")
        UIManager:close(picker)
        assert(dictionary.tag == "original" and #calls == 0)
        input.buttons[1][1].callback()
        UIManager.last.buttons[2][1].callback()
        assert(dictionary.tag == "harbour" and input.closed)
        assert(calls[1][1] == "beacon" and calls[1][2])
        calls[1][6]()
        assert(dictionary.tag == "original")

        for _, fuzzy in ipairs({false, true}) do
            local restored = false
            dictionary:showNoResultsDialog("beacno", {}, fuzzy, "rect", "link",
                function() assert(dictionary.tag == "original"); restored = true end,
                function() error("selection cancelled") end)
            local miss = UIManager.last
            assert(miss.input == "beacno" and miss.keyboard)
            miss.input = "beacon"
            local preset = fuzzy and miss.buttons[1][2] or miss.buttons[1][1]
            assert(preset.text == "Search with preset")
            preset.callback()
            UIManager.last.buttons[1][1].callback()
            assert(dictionary.tag == "coast" and miss.closed)
            local call = calls[#calls]
            assert(call[1] == "beacon" and call[3] == "rect" and call[4] == "mark" and call[5] == "link")
            call[6]()
            assert(restored)
        end

        local event
        local window = setmetatable({
            ui = { dictionary = dictionary, handleEvent = function(_, e) event = e end },
        }, { __index = DictQuickLookup })
        window:lookupInputWord("harbour")
        local editor = UIManager.last
        editor.buttons[1][2].callback()
        UIManager.last.buttons[2][1].callback()
        assert(dictionary.tag == "harbour" and editor.closed)
        assert(event[1] == "LookupWord" and event[2] == "harbour" and event[3])
        event[7]()
        assert(dictionary.tag == "original")
    """)
    print(f"KOReader {REFERENCE} Lua 5.1: typed/cancel, miss, popup edit, and close restoration passed")


if __name__ == "__main__":
    main()
