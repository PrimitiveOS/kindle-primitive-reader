#!/usr/bin/env python3
"""Exercise stock dictionary result callbacks with an authored result list."""
import argparse
import hashlib
from pathlib import Path

from lupa.lua51 import LuaRuntime

PIN = "9192014d8bd82a91dc1012473be0f238dedfdb54"
SHA256 = "1097fe3fbf4ef26c4c1b259d044af72f8258523537d73ad843ec5ce3a7c281bd"

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("source", type=Path, help="pinned frontend/ui/widget/dictquicklookup.lua")
args = parser.parse_args()
raw = args.source.read_bytes()
if hashlib.sha256(raw).hexdigest() != SHA256:
    parser.error(f"Use the unmodified dictquicklookup.lua from KOReader {PIN}")
source = raw.decode("utf-8")

lua = LuaRuntime(unpack_returned_tuples=True)
lua.execute("""
    DictQuickLookup = {}
    _ = function(text) return text end
    T = function(template, ...) return template end
""")
start = source.index("function DictQuickLookup:isPrevDictAvaiable()")
end = source.index("\n--[[ No longer used", start)
lua.execute(source[start:end])
start = source.index("        prev_dict = {")
end = source.index("        wikipedia = {", start)
lua.globals().buttons = lua.eval("function(self) return {" + source[start:end] + "} end")

compare = lua.eval("""function()
    local popup = setmetatable({
        dict_index = 1,
        word = "beacon",
        results = {
            {dict = "First", word = "beacon", definition = "Visible marker."},
            {dict = "Middle", word = "beacon", definition = "Navigational light."},
            {dict = "Last", word = "beacon", definition = "Radio signal."},
        },
        save_highlight = true,
        update = function() end,
        isDocless = function() return false end,
    }, {__index = DictQuickLookup})
    local function at(index)
        assert(popup.dict_index == index)
        assert(popup.dictionary == popup.results[index].dict)
        assert(popup.save_highlight)
    end
    popup:changeDictionary(1)
    assert(not buttons(popup).prev_dict.enabled)
    buttons(popup).prev_dict.hold_callback()
    at(1)
    buttons(popup).next_dict.callback()
    at(2)
    buttons(popup).next_dict.hold_callback()
    at(3)
    assert(not buttons(popup).next_dict.enabled)
    buttons(popup).next_dict.hold_callback()
    at(3)
    buttons(popup).prev_dict.hold_callback()
    at(1)
    return true
end""")

assert compare()
print(f"KOReader {PIN} on {lua.lua_implementation}: tap steps, holds jump, and the armed highlight survives")
