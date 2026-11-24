#!/usr/bin/env python3
"""Run the pinned KOReader dictionary retry callbacks with a UI test double."""
import argparse
import hashlib
from pathlib import Path

from lupa.lua51 import LuaRuntime

PIN = "9192014d8bd82a91dc1012473be0f238dedfdb54"
SHA256 = "8ca93c7881b7bca33d9afb076e32e85bf203543d80a0b1e053910667b6df79f9"

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("source", type=Path, help="pinned frontend/apps/reader/modules/readerdictionary.lua")
args = parser.parse_args()
raw = args.source.read_bytes()
if hashlib.sha256(raw).hexdigest() != SHA256:
    parser.error(f"Use the unmodified readerdictionary.lua from KOReader {PIN}")
source = raw.decode("utf-8")
start = source.index("function ReaderDictionary:showNoResultsDialog(")
end = source.index("\nfunction ReaderDictionary:showDownload(", start)

lua = LuaRuntime()
lua.execute("""
ReaderDictionary = {dismissLookupInfo = function(self) self.dismissed = true end}
_ = function(text) return text end
Presets = {getPresets = function() return {} end}
UIManager = {
    show = function(self, dialog) self.shown = dialog end,
    close = function(self, dialog) self.closed = dialog end,
    scheduleIn = function(_, _, task) task() end,
}
InputDialog = {new = function(_, dialog)
    function dialog:onShowKeyboard() self.keyboard = true end
    function dialog:getInputText() return self.input end
    return dialog
end}
G_defaults = {readSetting = function() return 0.1 end}
""")
lua.execute(source[start:end])
lua.execute("""
local dictionary = ReaderDictionary
local names, boxes, link = {"Harbour"}, {7}, "link"
local cancelled = false
local close = function() end
local cancel = function() cancelled = true end
function dictionary:stardictLookup(word, sources, fuzzy, selection, target, on_close)
    assert(word == "beacno" and fuzzy and sources == names)
    assert(selection == boxes and target == link and on_close == close)
    self.looked_up = true
end

assert(dictionary:showNoResultsDialog("beacnox", names, false, boxes, link, close, cancel))
local dialog = UIManager.shown
assert(dialog.input == "beacnox" and dialog.keyboard and dictionary.dismissed)
assert(dialog.buttons[1][2].is_enter_default)
dialog.input = ""
dialog.buttons[1][2].callback()
assert(not dictionary.looked_up and not UIManager.closed)
dialog.input = "beacno"
dialog.buttons[1][2].callback()
assert(dictionary.looked_up and UIManager.closed == dialog and not cancelled)

assert(dictionary:showNoResultsDialog("beacnox", names, false, boxes, link, close, cancel))
UIManager.shown.buttons[1][1].callback()
assert(cancelled and UIManager.closed == UIManager.shown)
assert(not dictionary:showNoResultsDialog("beacno", names, true, boxes, link, close, cancel))
""")
print(f"KOReader {PIN} on {lua.lua_implementation}: prefilled retry, Enter, selection context, Cancel and exhaustion passed")
