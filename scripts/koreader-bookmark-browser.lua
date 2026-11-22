-- Exercise the installed bookmark browser's data methods without opening its UI.
-- Run from the matching KOReader directory with its own LuaJIT and shared libraries.
package.path = "./frontend/?.lua;./?.lua;" .. package.path
require("setupkoenv")
local checked = 0
local function expect(value)
    assert(value)
    checked = checked + 1
end
local function widget()
    local base = {}
    function base:extend(fields)
        return setmetatable(fields or {}, { __index = self })
    end
    function base:new(fields)
        return self:extend(fields)
    end
    return base
end
local shown
local ui_manager = {
    show = function(_, value) shown = value end,
    close = function() end,
}
local bookmark = { display_prefix = {}, display_type = {}, separator = " | " }
local file = assert(io.open("frontend/apps/reader/modules/readerbookmark.lua", "rb"))
local bookmark_source = file:read("*a")
file:close()
local kind_function = assert(bookmark_source:match("function ReaderBookmark%.getBookmarkType.-\nend"))
local environment = { ReaderBookmark = bookmark }
local classify = assert(loadstring(kind_function))
setfenv(classify, setmetatable(environment, { __index = _G }))
classify()
function bookmark:getBookmarkItemText(item) return item.text_orig end
function bookmark:getBookmarkDetailsText(item) return item.text_orig end
local class = widget()
local stubs = {
    ["ui/bidi"] = { getArrowLabels = function() return "Previous", "Next", "First", "Last" end },
    ["ffi/blitbuffer"] = {},
    ["apps/reader/modules/readerbookmark"] = bookmark,
    ["apps/reader/modules/readerhighlight"] = {},
    ["ui/uimanager"] = ui_manager,
    ["ui/widget/container/widgetcontainer"] = class,
    ["device"] = { screen = {} },
}
for _, name in ipairs({
    "ui/widget/booklist", "ui/widget/buttondialog", "ui/widget/buttonselector",
    "ui/widget/checkbutton", "docsettings", "document/documentregistry",
    "ui/widget/infomessage", "ui/widget/inputdialog", "ui/widget/menu",
    "ui/widget/pathchooser", "ui/widget/textviewer", "apps/filemanager/filemanagerutil",
}) do stubs[name] = widget() end
for name, value in pairs(stubs) do package.loaded[name] = value end
local browser_class = assert(loadfile("frontend/ui/widget/bookmarkbrowser.lua"))()
local a = {
    file = "/authored/first.epub", enabled = true, authors = "Aria Coast",
    doc_props = { display_title = "Field report" },
    annotations = {
        { page = 1, text = "At sunrise", datetime = "2025-03-01 08:00:00" },
        { page = 4, text = "Copper lantern", note = "Review the amber chart",
          drawer = "underscore", datetime = "2025-02-01 08:00:00" },
    },
}
local b = {
    file = "/authored/second.pdf", enabled = true, authors = "Zani Rivers",
    doc_props = { display_title = "Log sheets" },
    annotations = {
        { page = 2, text = "Harbor sounding", drawer = "lighten",
          datetime = "2025-01-01 08:00:00" },
    },
}
local browser = browser_class:new {
    books = { a, b }, filter_table = { enabled_books_nb = 2 },
    display_prefix = {}, display_type = {}, sorting_mode = "page",
}
local function rows()
    local result = browser:getItemTable()
    for index, item in ipairs(result) do item.idx = index end
    return result
end
local items = rows()
expect(#items == 5 and browser.visible_books_nb == 2)
expect(items[2].type == "bookmark" and items[3].type == "note" and items[5].type == "highlight")
browser.sorting_mode = "date"
items = rows()
expect(items[2].text_orig == "Copper lantern" and items[3].text_orig == "At sunrise")
browser.is_reverse_sorting = true
items = rows()
expect(items[2].text_orig == "At sunrise")
for _, filter in ipairs({
    { type = "note", enabled_books_nb = 2 },
    { style = "underscore", enabled_books_nb = 2 },
    { search_string = "AMBER", enabled_books_nb = 2 },
}) do
    browser.filter_table = filter
    items = rows()
    expect(#items == 2 and items[2].text_orig == "Copper lantern")
end
browser.filter_table = { search_string = "AMBER", search_case_sensitive = true, enabled_books_nb = 2 }
expect(#rows() == 0)
browser.filter_table = { search_string = "lantern Review", enabled_books_nb = 2 }
expect(#rows() == 0)
a.annotations[2].note = "Review the Ámber chart"
browser.filter_table = { search_string = "ámber", enabled_books_nb = 2 }
expect(#rows() == 2)
a.enabled = false
browser.filter_table = { enabled_books_nb = 1 }
items = rows()
expect(#items == 2 and items[2].text_orig == "Harbor sounding")
a.enabled = true
browser.filter_table = { enabled_books_nb = 2 }
items = rows()
browser.bm_list = { item_table = items, onClose = function() end }
function browser:updateBookmarkList(index) self.selected = index end
browser:showBookmarkDetails(items[3])
shown.buttons_table[1][3].callback()
expect(browser.selected == 5)
shown.buttons_table[1][2].callback()
expect(browser.selected == 3)
shown.buttons_table[1][1].callback()
expect(browser.selected == 2)
shown.buttons_table[1][4].callback()
expect(browser.selected == 5)
local calls = {}
browser.ui = {
    document = { file = a.file },
    switchDocument = function(_, path, _, after)
        calls[#calls + 1] = "open"
        expect(path == b.file)
        after {
            link = { addCurrentLocationToStack = function() calls[#calls + 1] = "history" end },
            bookmark = { gotoBookmark = function(_, page)
                calls[#calls + 1] = "jump"
                expect(page == 2)
            end },
        }
    end,
}
browser:showBookmarkDetails(items[#items])
expect(#calls == 0 and shown.buttons_table[2][1].enabled)
shown.onClose = function() end
shown.buttons_table[2][1].callback()
expect(table.concat(calls, ",") == "open,history,jump")
b.is_deleted = true
b.doc_settings = { readSetting = function() return { on_closing = false } end }
items = rows()
browser.bm_list.item_table = items
browser:showBookmarkDetails(items[#items])
expect(not shown.buttons_table[2][1].enabled)
print(string.format('{"bookmark_browser_checks":%d,"real_methods":true,"foreground_opened":false}', checked))
