#!/usr/bin/env python3
"""Run pinned KOReader per-book dictionary exclusion, lookup and reset callbacks."""
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

lua = LuaRuntime()
lua.execute("""
_ = function(text) return text end
local settings = {}
function settings:readSetting(key) return self.data[key] end
function settings:saveSetting(key, value) self.data[key] = value end
function settings:delSetting(key) self.data[key] = nil end
function settings:has(key) return self.data[key] ~= nil end
function settings:isTrue(key) return self.data[key] == true end
function new_settings(data) return setmetatable({data = data}, {__index = settings}) end
G_reader_settings = new_settings({dicts_disabled = {['Harbour.ifo'] = true}})
local edition = new_settings({preferred_dictionaries = {'Harbour'}})
ReaderDictionary = {
    ui = {doc_settings = edition, handleEvent = function() end},
    disable_lookup_history = true,
    showLookupInfo = function() end,
    showDict = function(self, _, results) self.results = results end,
    startSdcv = function(self, _, names)
        self.searched = table.concat(names, ',')
        return {{dict = names[1]}}
    end,
}
available_ifos = {
    {name = 'Harbour', file = 'Harbour.ifo'},
    {name = 'Reference', file = 'Reference.ifo'},
}
ButtonDialog = {new = function(_, dialog) return dialog end}
UIManager = {
    show = function(self, dialog) self.dialog = dialog end,
    close = function() end,
    getTime = function() return 0 end,
}
util = {unicodeCodepointToUtf8 = function() return '#' end}
Event = {new = function() end}
Device = {canExternalDictLookup = function() return false end}
tidyMarkup = function(results) return results end
""")
for method in (
    "updateSdcvDictNamesOptions",
    "showPreferredDictsDialog",
    "stardictLookup",
    "onReadSettings",
    "onSaveSettings",
):
    start = source.index(f"function ReaderDictionary:{method}(")
    end = source.index("\nfunction ", start + 1)
    lua.execute(source[start:end])

lua.execute("""
local d = ReaderDictionary
local edition = d.ui.doc_settings
local menu = {updateItems = function() end}
d:onReadSettings(edition)
d:showPreferredDictsDialog(menu)
d:stardictLookup('tide', d.enabled_dict_names, false)
assert(d.searched == 'Harbour,Reference')

UIManager.dialog.buttons[1][1].hold_callback()
assert(edition:readSetting('disabled_dicts').Harbour)
assert(d.preferred_dictionaries[1] == 'Harbour')
d:onReadSettings(edition)
d:stardictLookup('tide', d.enabled_dict_names, false)
assert(d.searched == 'Reference')
UIManager.dialog.buttons[2][1].hold_callback()
d:stardictLookup('tide', d.enabled_dict_names, false)
assert(d.searched == 'Harbour,Reference')

UIManager.dialog.buttons[2][1].hold_callback()
UIManager.dialog.buttons[1][1].hold_callback()
d.searched = nil
d:stardictLookup('tide', d.enabled_dict_names, true)
assert(d.searched == nil and d.results[1].no_result)
UIManager.dialog.buttons[#UIManager.dialog.buttons][1].callback()
assert(edition:readSetting('preferred_dictionaries') == nil)
assert(edition:readSetting('disabled_dicts') == nil)
assert(G_reader_settings:readSetting('dicts_disabled')['Harbour.ifo'])
d:stardictLookup('tide', d.enabled_dict_names, true)
assert(d.searched == 'Reference')
""")
print(f"KOReader {PIN} on {lua.lua_implementation}: exclusion veto, reopen, restored priority, empty scope and reset passed")
