#!/usr/bin/env python3
"""Execute pinned KOReader's footnote font switch and face selection in Lua."""
import argparse
import hashlib
from pathlib import Path

from lupa.lua51 import LuaRuntime

PIN = "9192014d8bd82a91dc1012473be0f238dedfdb54"
HASHES = {
    "readerlink.lua": "7fce496dc92e02769183dd275d9636bbc81c5ef148c733c7895c71b92eea73e1",
    "footnotewidget.lua": "7b1bc436530224e8a86fc91eeaa7f9191a5ae6bab4aa61b34675f8675e8c5efc",
}

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("sources", type=Path, help="directory with the two pinned Lua files")
args = parser.parse_args()


def load(name):
    raw = (args.sources / name).read_bytes()
    if hashlib.sha256(raw).hexdigest() != HASHES[name]:
        parser.error(f"Use the unmodified {name} from KOReader {PIN}")
    return raw.decode("utf-8")


link = load("readerlink.lua")
widget = load("footnotewidget.lua")
lua = LuaRuntime(unpack_returned_tuples=True)
lua.execute("""
ReaderLink = {}
local preferences = {}
G_reader_settings = {
    isTrue = function(_, key) return preferences[key] == true end,
    flipNilOrFalse = function(_, key) preferences[key] = not preferences[key] end,
}
Device = {isTouchDevice = function() return true end}
Screen = {scaleBySize = function(_, value) return value end}
InputContainer = {extend = function(_, attrs) return attrs end}
_ = function(value) return value end
tap_links, swipe_links, previews = true, false, true
isTapToFollowLinksOn = function() return tap_links end
isSwipeToFollowNearestLinkEnabled = function() return swipe_links end
isFootnoteLinkInPopupEnabled = function() return previews end
isPreferFootnoteEnabled = function() return false end
local real_require = require
require = function(module)
    if module ~= "document/credocument" then return real_require(module) end
    return {engineInit = function()
        return {getFontFaceFilenameAndFaceIndex = function(name, bold, italic)
            calls = calls + 1
            if name ~= "Configured reading font" then return nil end
            return bold and "book-bold.ttf" or "book-regular.ttf", 0
        end}
    end}
end
T = function(pattern, ...)
    local args = {...}
    return (pattern:gsub("%%(%d)", function(i) return args[tonumber(i)] end))
end
calls = 0
""")

start = link.index("function ReaderLink:getFootnoteSettingsMenuTable()")
end = link.index("\nfunction ReaderLink:addToMainMenu", start)
lua.execute(link[start:end])
setting = lua.execute("""
    for _, item in ipairs(ReaderLink:getFootnoteSettingsMenuTable()[2].sub_item_table) do
        if item.text == "Use book font in popups" then return item end
    end
""")
start = widget.index("local FootnoteWidget = InputContainer:extend")
end = widget.index("\nfunction FootnoteWidget:init()", start)
assert lua.execute(widget[start:end] + "\nreturn FootnoteWidget.font_face") == "Noto Sans"
start = widget.index('    local font_css = ""')
end = widget.index('    local text_align_css = ""', start)
font = lua.eval("function(name) local self = {doc_font_name = name}\n" + widget[start:end] + "return font_css end")

assert setting.enabled_func()
assert not setting.checked_func()
assert font("Configured reading font") == ""
assert lua.globals().calls == 0
setting.callback()
assert setting.checked_func()
css = font("Configured reading font")
assert css.count("@font-face") == 2
assert "book-regular.ttf" in css and "book-bold.ttf" in css
assert "KOReaderFootnoteFont" in css
assert lua.globals().calls == 4
assert font("Unknown face") == ""
setting.callback()
assert not setting.checked_func()
lua.execute("tap_links = false")
assert not setting.enabled_func()
lua.execute("swipe_links = true")
assert setting.enabled_func()
lua.execute("previews = false")
assert not setting.enabled_func()

print(f"KOReader {PIN} on {lua.lua_implementation}: popup default uses Noto Sans; the saved switch enables configured face lookup")
