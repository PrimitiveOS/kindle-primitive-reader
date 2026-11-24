-- Run in the pinned Mac KOReader runtime with source, cache, font, and both authored EPUBs.
require("setupkoenv")
G_defaults = require("luadefaults"):open()
local DataStorage = require("datastorage")
G_reader_settings = require("luasettings"):open(DataStorage:getDataDir() .. "/settings.reader.lua")
local Device = require("device")
require("document/canvascontext"):init(Device)
local Cre = require("libs/libkoreader-cre")
local source, cache, font, headings, fragments = assert(arg[1]), assert(arg[2]), assert(arg[3]), assert(arg[4]), assert(arg[5])
Cre.initCache(cache, 0, true, 1)
Cre.registerFont(font)

local function script_at(path)
    local file = assert(io.open(path, "r"))
    local script = file:read("*a")
    file:close()
    return script
end
local script = script_at(source)
local function segment(first, last, unit)
    unit = unit or script
    local at = assert(unit:find(first, 1, true))
    return unit:sub(at, assert(unit:find(last, at + #first, true)) - 1)
end
local function open(path)
    local view = Cre.newDocView(1264, 1606, 1)
    view:setStyleSheet("./data/epub.css", "")
    view:setVisiblePageCount(1, false)
    view:setFontFace("Noto Sans")
    assert(view:loadDocument(path, false))
    view:renderDocument()
    return view
end

local popup, scheduled, event, shown, footer, closed, invalidated
local env = setmetatable({
    _ = function(text) return text end,
    ReaderToc = {}, ReaderFooter = {}, ReaderRolling = {},
    Screen = { scaleBySize = function(_, size) return size end },
    Size = { line = { medium = 2 } },
    ProgressWidget = { new = function(_, options) return options end },
    UIManager = {
        show = function(_, visual) popup = visual end,
        scheduleIn = function(_, seconds, run) assert(seconds == 0.5) scheduled = run end,
    },
    ConfirmBox = { new = function(_, options) return options end },
    Event = { new = function(_, kind) return kind end },
}, {__index = _G})
local function pinned(chunk)
    return setfenv(assert(loadstring(chunk)), env)()
end
local menu = pinned("return function(self, menu_items)\n" .. segment(
    "    if self.ui.document:canHaveAlternativeToc() then\n        menu_items.toc_alt_toc",
    "    -- Allow to have getTocTicksFlattened"
) .. "end")
pinned(segment("function ReaderToc:fillToc()", "function ReaderToc:validateAndFixToc()"))
pinned(segment("function ReaderToc:cleanUpTocTitle(", "function ReaderToc:onSetDimensions("))
pinned(segment("function ReaderToc:getTocIndexByPage(", "function ReaderToc:getFullTocTitleByPage("))
pinned(segment("function ReaderToc:getTocTicks(", "function ReaderToc:getPagePagemapIndex("))
pinned(segment("function ReaderToc:getChapterPageCount(", "function ReaderToc:getChapterPagesDone("))
local modules = assert(source:match("^(.*/)readertoc%.lua$"))
local footer_chapter = pinned("return {" .. segment(
    "    book_chapter = function(footer)", "    custom_text = function(footer)",
    script_at(modules .. "readerfooter.lua")) .. "}").book_chapter
local function footer_title(reader, page)
    return footer_chapter({ui = {toc = reader}, pageno = page,
        settings = {book_chapter_max_width_pct = 99},
        getFittedText = function(_, title, _) return title end})
end
pinned(segment("function ReaderFooter:setTocMarkers(", "function ReaderFooter:onUpdateFooter(",
    script_at(modules .. "readerfooter.lua")))
pinned(segment("function ReaderRolling:onUpdateTopStatusBarMarkers(", "function ReaderRolling:updateBatteryState(",
    script_at(modules .. "readerrolling.lua")))
local skim = pinned("return function(self)\n" .. segment(
    "    self.progress_bar = ProgressWidget:new{", "    -- Bottom row buttons",
    script_at(modules .. "../../../ui/widget/skimtowidget.lua")) .. "end")
local function same(actual, expected)
    assert(#actual == #expected)
    for i, value in ipairs(expected) do assert(actual[i] == value) end
end

local function comparison(path, expected)
    local view = open(path)
    local settings = {}
    local custom = false
    local reader = setmetatable({
        alt_toc_symbol = "#", toc_ticks_ignored_levels = {2},
        resetToc = function(self)
            self.toc, self.ticks, self.ticks_flattened, self.ticks_flattened_filtered = nil, nil, nil, nil
        end,
        onShowToc = function() shown = true end,
        validateAndFixToc = function() end,
        view = {footer = {
            setTocMarkers = function(_, enabled) assert(enabled) end,
            onUpdateFooter = function() footer = true end,
        }},
        ui = {
            document = {
                canHaveAlternativeToc = function() return true end,
                isTocAlternativeToc = function() return view:isTocAlternativeToc() end,
                buildAlternativeToc = function() view:buildAlternativeToc() end,
                getToc = function() return view:getToc() end,
                invalidateCacheFile = function() invalidated = true end,
            },
            doc_settings = {
                isTrue = function(_, name) return settings[name] == true end,
                delSetting = function(_, name) settings[name] = nil end,
                makeTrue = function(_, name) settings[name] = true end,
            },
            handmade = {isHandmadeTocEnabled = function() return custom end},
            handleEvent = function(_, name) event = name end,
            reloadDocument = function() view:close() view = open(path) end,
        },
    }, {__index = env.ReaderToc})
    local items = {}
    menu(reader, items)
    local toggle = assert(items.toc_alt_toc)
    assert(toggle.enabled_func() and not toggle.checked_func())
    custom = true
    assert(not toggle.enabled_func())
    custom = false
    local menu_instance = {closeMenu = function() closed = true end}
    toggle.callback(menu_instance)
    assert(settings.alternative_toc == nil)
    popup.ok_callback()
    assert(settings.alternative_toc and closed and shown and footer)
    assert(event == "UpdateTopStatusBarMarkers" and #reader.toc_ticks_ignored_levels == 0)
    assert(toggle.checked_func())
    reader:fillToc()
    assert(#reader.toc == #expected)
    for index, entry in ipairs(reader.toc) do
        local title = reader:cleanUpTocTitle(entry.title, true)
        print(index, entry.depth, title, entry.xpointer)
        assert(entry.depth == expected[index][1] and title == expected[index][2])
    end
    if #expected == 4 then
        local page = reader.toc[1].page
        assert(page > 1 and reader.toc[2].xpointer == reader.toc[3].xpointer)
        assert(reader.toc[2].title == "" and reader.toc[4].page == page)
        assert(footer_title(reader, 1) == "" and footer_title(reader, page) == "Harbor Survey")
        assert(footer_title(reader, page + 1) == "West quay")
        reader.toc_ticks_ignored_levels = {[1] = true}
        reader.toc_chapter_title_bind_to_ticks = true
        assert(footer_title(reader, page) == "")
        reader.toc_ticks_ignored_levels[2] = true
        assert(footer_title(reader, page) == "East softhyphen quay")
        reader.toc_chapter_title_bind_to_ticks = nil
    else
        assert(footer_title(reader, reader.toc[1].page) == "")
    end
    local function check_markers(ignored)
        reader.toc_ticks_ignored_levels = ignored
        reader.ticks_flattened_filtered = nil
        local pages, seen = {}, {}
        for _, entry in ipairs(reader.toc) do
            if not ignored[entry.depth] and not seen[entry.page] then
                pages[#pages + 1] = entry.page
                seen[entry.page] = true
            end
        end
        table.sort(pages)
        same(reader:getTocTicksFlattened(), pages)
        local header
        local doc = {
            hasHiddenFlows = function() return false end,
            getPageCount = function() return view:getPages() end,
            setHeaderProgressMarks = function(_, count, ticks)
                assert(count == view:getPages())
                header = ticks
            end,
        }
        local ui = {toc = reader, document = doc}
        env.ReaderRolling.onUpdateTopStatusBarMarkers({ui = ui, cre_top_bar_enabled = true})
        same(header, pages)
        local footer_state = {ui = ui, pages = view:getPages(), view = {view_mode = "page"},
            settings = {toc_markers = true, toc_markers_width = 3}, progress_bar = {ticks = {999}}}
        assert(env.ReaderFooter.setTocMarkers(footer_state, true))
        same(footer_state.progress_bar.ticks, pages)
        local skim_state = {ui = ui, curr_page = 1, page_count = view:getPages()}
        skim(skim_state)
        same(skim_state.progress_bar.ticks, pages)
        assert(skim_state.progress_bar.last == view:getPages())
    end
    check_markers({})
    check_markers({[2] = true})
    -- An ordinary reopen has no alternative cache here. fillToc rebuilds from its saved preference.
    view:close()
    view = open(path)
    assert(not toggle.checked_func())
    reader:resetToc()
    reader:fillToc()
    assert(toggle.checked_func() and #reader.toc == #expected)
    toggle.callback(menu_instance)
    popup.ok_callback()
    assert(settings.alternative_toc == nil and invalidated)
    assert(scheduled)
    scheduled()
    assert(not toggle.checked_func() and view:getToc()[1].title == "Publisher preface")
    view:close()
end

comparison(headings, {{1, "Harbor Survey"}, {2, "–"}, {3, "East softhyphen quay"}, {2, "West quay"}})
comparison(fragments, {{1, "–"}, {1, "–"}, {1, "–"}})
local chapter = setmetatable({
    toc = {{title = "Harbor", depth = 1, page = 3}, {title = "", depth = 2, page = 6},
        {title = "East", depth = 3, page = 6}, {title = "West", depth = 2, page = 9}},
    toc_ticks_ignored_levels = {[2] = true},
    ui = {document = {getPageCount = function() return 12 end,
        hasHiddenFlows = function() return false end}},
}, {__index = env.ReaderToc})
assert(footer_title(chapter, 6) == "" and footer_title(chapter, 7) == "East")
assert(chapter:getChapterPageCount(1) == 2 and chapter:getChapterPageCount(6) == 3)
assert(chapter:getChapterPagesLeft(6) == 2 and chapter:getNextChapter(6) == 9)
chapter.toc_chapter_title_bind_to_ticks = true
assert(footer_title(chapter, 6) == "East" and chapter:getNextChapter(6) == 9)
chapter.toc_chapter_navigation_bind_to_ticks = true
assert(chapter:getNextChapter(6) == nil and chapter:getChapterPagesLeft(6) == nil)
assert(chapter:getPreviousChapter(10) == 6 and chapter:getChapterPageCount(6) == 7)
print("Pinned KOReader: alternative Contents, title order, independent depth choices, chapter spans, and shared header/footer/Skim ticks passed")
Device:exit()
