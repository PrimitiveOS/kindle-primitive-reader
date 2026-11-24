#!/usr/bin/env python3
"""Run pinned KOReader dictionary membership and name-management callbacks."""
import argparse
import hashlib
from pathlib import Path

from lupa.lua51 import LuaRuntime

PIN = "9192014d8bd82a91dc1012473be0f238dedfdb54"
SOURCES = {
    "apps/reader/modules/readerdictionary.lua": "8ca93c7881b7bca33d9afb076e32e85bf203543d80a0b1e053910667b6df79f9",
    "ui/presets.lua": "8a681d3adb5ff96c27612505afebf3054ef64af2353a425e409b409bf407049a",
}
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("frontend", type=Path, help="pinned KOReader frontend directory")
args = parser.parse_args()
loaded = {}
for relative, checksum in SOURCES.items():
    raw = (args.frontend / relative).read_bytes()
    if hashlib.sha256(raw).hexdigest() != checksum:
        parser.error(f"Use the unmodified {relative} from KOReader {PIN}")
    loaded[relative] = raw.decode("utf-8")

lua = LuaRuntime()
lua.execute("""
local function widget()
    return {new = function(_, spec)
        function spec:getInputText() return self.input end
        function spec:onShowKeyboard() self.keyboard = true end
        return spec
    end}
end
for _, name in ipairs({'confirmbox', 'infomessage', 'inputdialog', 'notification'}) do
    package.preload['ui/widget/' .. name] = widget
end
local notifications = {}
package.preload['ui/widget/notification'] = function()
    return {
        notices = notifications,
        notify = function(self, message) table.insert(self.notices, message) end,
    }
end
package.preload['ui/event'] = function() return {new = function() end} end
package.preload['ui/uimanager'] = function()
    return {
        notices = {},
        show = function(self, widget)
            self.last = widget
            if widget.text then table.insert(self.notices, widget.text) end
        end,
        close = function() end,
        broadcastEvent = function() end,
    }
end
package.preload['ffi/util'] = function()
    return {
        orderedPairs = function(items)
            local keys = {}
            for key in pairs(items) do table.insert(keys, key) end
            table.sort(keys)
            local index = 0
            return function()
                index = index + 1
                return keys[index]
            end
        end,
        template = function(message, value) return (message:gsub('%%1', value)) end,
    }
end
package.preload['gettext'] = function() return function(text) return text end end
_ = require('gettext')
InfoMessage = require('ui/widget/infomessage')
UIManager = require('ui/uimanager')
local config = {dicts_disabled = {['Reference.ifo'] = true}}
G_reader_settings = {
    readSetting = function(_, key) return config[key] end,
    saveSetting = function(_, key, value) config[key] = value end,
}
ReaderDictionary = {onSaveSettings = function() end}
util = {tableSize = function(items)
    local count = 0
    for _ in pairs(items) do count = count + 1 end
    return count
end}
available_ifos = {
    {name = 'Harbour', file = 'Harbour.ifo'},
    {name = 'Reference', file = 'Reference.ifo'},
}
""")
lua.globals().Presets = lua.execute(loaded["ui/presets.lua"])
source = loaded["apps/reader/modules/readerdictionary.lua"]
for name in ("updateSdcvDictNamesOptions", "buildPreset", "loadPreset", "onCycleDictionaryPresets"):
    start = source.index(f"function ReaderDictionary:{name}(")
    end = source.index("\nfunction ", start + 1)
    lua.execute(source[start:end])

lua.execute("""
local dictionary = ReaderDictionary
local presets = {
    presets = {},
    buildPreset = function() return dictionary:buildPreset() end,
    loadPreset = function(state) dictionary:loadPreset(state) end,
}
local menu = {updateItems = function() end}
local function new_items()
    menu.item_table = Presets.genPresetMenuItemTable(presets, nil,
        function() return #dictionary.enabled_dict_names > 0 end)
    return menu.item_table
end
dictionary:updateSdcvDictNamesOptions()
assert(new_items()[1].enabled_func())
menu.item_table[1].callback(menu)
assert(UIManager.last.keyboard)
UIManager.last.input = 'Local tides'
UIManager.last.buttons[1][2].callback()
assert(presets.presets['Local tides'].enabled_dict_names.Harbour)
assert(not presets.presets['Local tides'].enabled_dict_names.Reference)

available_ifos = {
    {name = 'Reference', file = 'Reference.ifo'},
    {name = 'New source', file = 'New.ifo'},
}
dictionary.preferred_dictionaries = {'Reference'}
menu.item_table[2].callback()
assert(dictionary.enabled_dict_names[1] == 'Reference' and #dictionary.enabled_dict_names == 1)
assert(G_reader_settings:readSetting('dicts_disabled')['New.ifo'])
assert(UIManager.notices[#UIManager.notices - 1]:find('Harbour', 1, true))

-- The confirmed update keeps membership only, even when the current set is empty.
dictionary.preferred_dictionaries = {}
dictionary:updateSdcvDictNamesOptions()
assert(not new_items()[1].enabled_func())
menu.item_table[2].hold_callback(menu, {idx = 2})
UIManager.last.ok_callback()
UIManager.last.ok_callback()
assert(next(presets.presets['Local tides'].enabled_dict_names) == nil)
menu.item_table[2].hold_callback(menu, {idx = 2})
UIManager.last.other_buttons[1][2].callback()
assert(UIManager.last.input == 'Local tides')
UIManager.last.input = 'English reference'
UIManager.last.buttons[1][2].callback()
assert(presets.presets['English reference'] and not presets.presets['Local tides'])
menu.item_table[2].hold_callback(menu, {idx = 2})
UIManager.last.other_buttons[1][1].callback()
UIManager.last.ok_callback()
assert(next(presets.presets) == nil)

local notices = require('ui/widget/notification').notices
dictionary.preset_obj = presets
local cycle_save_count = 0
function presets:saveCycleIndex()
    cycle_save_count = cycle_save_count + 1
    G_reader_settings:saveSetting('dict_presets_cycle_index', self.cycle_index)
end
assert(dictionary:onCycleDictionaryPresets())
assert(notices[#notices] == 'No presets available' and cycle_save_count == 0)
presets.presets.Zulu = {enabled_dict_names = {['New source'] = true}}
presets.presets.Everyday = {enabled_dict_names = {Reference = true}}
assert(Presets.onLoadPreset(presets, 'Zulu', true))
assert(presets.cycle_index == nil and dictionary.enabled_dict_names[1] == 'New source')
assert(dictionary:onCycleDictionaryPresets())
assert(presets.cycle_index == 1 and dictionary.enabled_dict_names[1] == 'Reference')
assert(notices[#notices] == 'Loaded preset: Everyday')
assert(dictionary:onCycleDictionaryPresets())
assert(G_reader_settings:readSetting('dict_presets_cycle_index') == 2)
assert(dictionary.enabled_dict_names[1] == 'New source')
table.insert(available_ifos, {name = 'Recently installed', file = 'Recent.ifo'})
assert(dictionary:onCycleDictionaryPresets())
assert(presets.cycle_index == 1 and dictionary.enabled_dict_names[1] == 'Reference')
assert(G_reader_settings:readSetting('dicts_disabled')['Recent.ifo'])
assert(cycle_save_count == 3)
""")
print(f"KOReader {PIN} on {lua.lua_implementation}: preset create, apply, missing and new sources, preferred override, empty update, rename, delete and cycle passed")
