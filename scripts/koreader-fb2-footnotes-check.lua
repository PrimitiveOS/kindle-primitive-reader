-- Run as a KO_HOME priority-2 patch with the pinned ReaderUI and workshop-notes.fb2.
local UIManager = require("ui/uimanager")
local ReaderUI = require("apps/reader/readerui")
local Device = require("device")
local evidence = assert(os.getenv("READER_FB2_STOCK_PROOF"))
local cases = {
    { id = "earlier-note", body = "notes", quote = "A second entry records the final measurement." },
    { id = "later-comment", body = "comments", quote = "The next entry explains how the panes dried." },
    { id = "earlier-detail" },
    { id = "plain-target" },
}
local index = 0
local function top()
    local windows = UIManager._window_stack
    return windows[#windows] and windows[#windows].widget
end
local function later(callback)
    UIManager:scheduleIn(0.25, function()
        local ok, problem = xpcall(callback, debug.traceback)
        if not ok then
            io.stderr:write(problem .. "\n")
            UIManager:quit(1)
        end
    end)
end

local function next_case()
    index = index + 1
    local case = cases[index]
    if not case then
        print("FB2 stock: two section popups, two ordinary links, section content and exact source positions")
        UIManager:quit(0)
        return
    end
    local ui = assert(ReaderUI.instance)
    local doc = ui.document
    -- Each case starts from the authored story, including the backwards target.
    ui.rolling:onGotoXPointer("#main-start")
    later(function()
        local found
        for _, item in ipairs(doc:getPageLinks(true)) do
            if item.section and item.section:sub(-#case.id) == case.id then
                found = item
                break
            end
        end
        assert(found, "No rendered link for " .. case.id)
        local point = {
            x = math.floor((found.start_x + found.end_x) / 2),
            y = math.floor((found.start_y + found.end_y) / 2),
        }
        local link = assert(ui.link:getLinkFromGes({ pos = point }))
        assert(link.xpointer == found.section)
        local accepted, reason, extension, first, last =
            doc:isLinkToFootnote(link.from_xpointer, link.xpointer, 0xdf7e, 10000)
        assert(accepted == (case.body ~= nil), case.id .. ": " .. tostring(reason))
        if accepted then
            assert(reason == "target is FB2 footnote (body[name=" .. case.body .. "] section)")
            assert(extension == "trusted container, no need" and not first and not last)
        end

        local before = doc:getXPointer()
        assert(ui.link:onTap(nil, { pos = point }))
        later(function()
            local popup = top()
            if accepted then
                assert(popup ~= ui and type(popup.html) == "string")
                local text = popup.html:gsub(string.char(194, 173), "")
                assert(text:find(case.quote, 1, true), popup.html)
                assert(not popup.html:find("Unrelated account", 1, true))
                assert(not popup.html:find("wooden crates", 1, true))
                assert(doc:getXPointer() == before)
                Device.screen:shot(evidence .. "/stock-fb2-" .. case.id .. ".png")
                UIManager:close(popup)
                assert(doc:getXPointer() == before)
            else
                assert(popup == ui or type(popup.html) ~= "string")
                assert(doc:getCurrentPage() == doc:getPageFromXPointer(link.xpointer))
            end
            print("FB2 " .. case.id .. ": " .. reason)
            later(next_case)
        end)
    end)
end

local attempts = 0
local function ready()
    attempts = attempts + 1
    local ui = ReaderUI.instance
    if not (ui and ui.document and ui.rolling and ui.link and top() == ui) then
        assert(attempts < 50, "The pinned FB2 ReaderUI did not settle")
        later(ready)
        return
    end
    assert(G_reader_settings:isTrue("footnote_link_in_popup"))
    assert(not G_reader_settings:isTrue("link_prefer_footnote"))
    assert(ui.document:isXPointerInDocument("#main-start"))
    assert(Device.screen:getWidth() == 1264 and Device.screen:getHeight() == 1680)
    next_case()
end
later(ready)
