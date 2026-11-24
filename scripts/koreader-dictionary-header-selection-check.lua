-- Run inside the pinned KOReader Mac runtime with a private KO_HOME.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
G_reader_settings = require("luasettings"):wrap({})
require("document/canvascontext"):init(require("device"))
local UIManager = require("ui/uimanager")
local Lookup = require("ui/widget/dictquicklookup")
local Highlight = require("apps/reader/modules/readerhighlight")

local saved, closed = 0, {}
local highlight = {
    saveHighlight = function() saved = saved + 1 end,
    clear = function() end,
}
local results = {
    {dict = "Harbour reference", word = "lighthouse", definition = "A visible coastal marker."},
    {dict = "Seafarer's glossary", word = "lighthouse", definition = "A navigational light. Another light marks its approach."},
    {dict = "Seafarer's glossary", word = "lighthouse", definition = "A building with a light visible from the sea."},
}
local function open(entries, name, selected)
    local popup = Lookup:new{
        results = entries,
        word = "lighthouses",
        highlight = selected,
        ui = {dictionary = {default_layout = {{"prev_dict", "next_dict", "close"}}}},
        dict_close_callback = function() table.insert(closed, name) end,
    }
    UIManager:show(popup)
    return popup
end

local popup = open(results, "origin", highlight)
local function position(index, count, dictionary)
    assert(popup.dict_index == index and popup.displaydictname == dictionary)
    assert(popup.lookup_word_text.text == "lighthouse")
    assert(popup.displaynb_text.text == index .. " / " .. count)
end
position(1, 3, "Harbour reference")
assert(not popup.button_table:getButtonById("prev_dict").enabled)
popup.button_table:getButtonById("next_dict").callback()
position(2, 3, "Seafarer's glossary")
popup:searchInDefinition("light")
assert(popup.in_definition_search)
popup.dict_title.close_callback()
assert(not popup.in_definition_search and #Lookup.window_list == 1)
position(2, 3, "Seafarer's glossary")
popup.button_table:getButtonById("next_dict").hold_callback()
position(3, 3, "Seafarer's glossary")
assert(not popup.button_table:getButtonById("next_dict").enabled)
popup.button_table:getButtonById("prev_dict").hold_callback()
position(1, 3, "Harbour reference")

popup.save_highlight = true
local nested = open({results[1]}, "one")
assert(nested.displaynb_text.text == "1 / 1")
nested.dict_title.close_callback()
assert(#Lookup.window_list == 1 and saved == 0 and closed[1] == "one")
nested = open({results[3]}, "stack")
nested.dict_title.close_hold_callback()
assert(#Lookup.window_list == 0 and saved == 1)
assert(table.concat(closed, ",") == "one,stack,origin")

local annotations = {}
local dismissed = false
local selection = Highlight:new{
    ui = {annotation = {annotations = annotations}, registerPostInitCallback = function() end},
    document = {info = {has_pages = false}},
    view = {highlight = {saved_drawer = "lighten"}},
    saveHighlight = function()
        table.insert(annotations, {text = "lighthouse"})
        return #annotations
    end,
    onClose = function() dismissed = true end,
}
local select = selection._highlight_buttons["01_select"](selection)
assert(select.text == "Select" and select.enabled and not selection.select_mode)
select.callback()
assert(selection.select_mode and selection.highlight_idx == 1 and dismissed)
assert(annotations[1].is_tmp and annotations[1].text == "lighthouse")
local extend = selection._highlight_buttons["01_select"](selection, 1)
assert(extend.text == "Extend" and extend.enabled)
annotations[1].text_edited = true
assert(not selection._highlight_buttons["01_select"](selection, 1).enabled)

print("Stock header, source and result count, arrow holds, title close and Select verified")
