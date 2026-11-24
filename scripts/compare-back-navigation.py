#!/usr/bin/env python3
"""Run the pinned KOReader Back routes with authored reading and folder histories."""
import argparse
import hashlib
from pathlib import Path

from lupa.lua51 import LuaRuntime


PIN = "9192014d8bd82a91dc1012473be0f238dedfdb54"
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("checkout", type=Path, help="directory containing pinned KOReader frontend")
checkout = parser.parse_args().checkout


def source(relative_path, checksum):
    raw = (checkout / relative_path).read_bytes()
    if hashlib.sha256(raw).hexdigest() != checksum:
        parser.error(f"Use {relative_path} from KOReader {PIN}")
    return raw.decode("utf-8")


reader = source(
    "frontend/apps/reader/modules/readerback.lua",
    "6992b529d2080b6a73a9cf4e92c6846685c868beaf6a5532f919c4ae647ebd09",
)
files = source(
    "frontend/apps/filemanager/filemanager.lua",
    "24c4deeba26888b820b064769607168732384a99d7e88b740bb136c68ac09f65",
)
lua = LuaRuntime(unpack_returned_tuples=True)
lua.execute("""
    local function identity(_, value) return value end
    local modules = {
        ["ui/widget/confirmbox"] = {new = identity},
        ["ui/widget/notification"] = {new = identity},
        ["ui/widget/eventlistener"] = {extend = identity},
        ["ui/event"] = {
            new = function(_, name, location) return {name = name, location = location} end,
        },
        ["ui/uimanager"] = {
            show = function(_, panel) visible = panel end,
            nextTick = function(_, callback) pending = callback end,
        },
        ["logger"] = {dbg = function() end},
        ["gettext"] = function(text) return text end,
    }
    function require(key) return modules[key] or {} end
    ConfirmBox = require("ui/widget/confirmbox")
    UIManager = require("ui/uimanager")
    _ = require("gettext")
    function runPending()
        if pending then
            local callback = pending
            pending = nil
            callback()
        end
    end
    settings = {}
    G_reader_settings = {
        readSetting = function(_, name, default) return settings[name] or default end,
    }
    FileChooser = {}
""")
lua.globals().ReaderBack = lua.execute(reader)
start = files.index("function FileChooser:onBack()")
end = files.index("function FileManager:onSwipeFM", start)
lua.execute(files[start:end])
lua.execute("""
    local page = "first"
    local jump = nil
    local latest_event = nil
    local link_selected = false
    local reader = setmetatable({
        ui = {
            document = {info = {has_pages = false}},
            rolling = {getBookLocation = function() return page end},
            link = {
                isPageLinkSelected = function() return link_selected end,
                clearSelectedPageLink = function() link_selected = false end,
                onGoBackLink = function(_, show_notification)
                    if jump then
                        page, jump = jump, nil
                        return true
                    end
                    if show_notification then visible = {text = "Location history is empty."} end
                end,
            },
            handleEvent = function(_, event)
                latest_event = event.name
                if event.name == "RestoreBookLocation" then page = event.location.xpointer end
            end,
        }
    }, {__index = ReaderBack})
    reader._addPreviousLocationToStackCallback = function() reader:_addPreviousLocationToStack() end
    reader:onReadSettings()

    reader:onBack()
    assert(visible.text == "Location history is empty.")
    reader:onBack()
    assert(visible.ok_text == "Exit" and latest_event == nil)
    visible.ok_callback()
    assert(latest_event == "Close")
    jump, page = "first", "second"
    reader:onBack()
    assert(page == "first" and not jump)
    link_selected, jump = true, "chapter"
    reader:onBack()
    assert(not link_selected and jump == "chapter")

    settings.back_in_reader = "previous_read_page"
    settings.back_to_exit = "always"
    reader:onReadSettings()
    reader:onReaderReady()
    runPending()
    page = "chapter"
    reader:onPageUpdate()
    runPending()
    page = "chapter_scrolled"
    reader:onPosUpdate()
    runPending()
    reader:onPosUpdate()
    runPending()
    assert(#reader.location_stack == 2)
    reader:onBack()
    runPending()
    assert(page == "chapter" and #reader.location_stack == 1)
    reader:onBack()
    runPending()
    assert(page == "first" and #reader.location_stack == 0)
    latest_event = nil
    reader:onBack()
    assert(latest_event == nil)
    reader:onBack()
    assert(latest_event == "Close")
    settings.back_to_exit = "disable"
    latest_event = nil
    reader:onBack()
    reader:onBack()
    assert(latest_event == nil and visible.text == "Location history is empty.")

    settings.back_in_reader = "filebrowser"
    reader:onBack()
    assert(latest_event == "Home")
    local folder = setmetatable({
        path = "Documents/Current",
        changeToPath = function(_, path) assert(path == "Documents/Current/..") end,
        onClose = function() latest_event = "File browser closed" end,
    }, {__index = FileChooser})
    settings.back_in_filemanager = "parent_folder"
    assert(folder:onBack())
    settings.back_in_filemanager, settings.back_to_exit = nil, nil
    assert(folder:onBack() and visible.ok_text == "Exit")
    visible.ok_callback()
    assert(latest_event == "File browser closed")
""")
print(f"KOReader {PIN} on {lua.lua_implementation}: separate histories, empty notice, exit choices, file browser and parent")
