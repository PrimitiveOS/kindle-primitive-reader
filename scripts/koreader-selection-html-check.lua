-- Install in a scratch KO_HOME patches directory and open the authored EPUB.
local UIManager = require("ui/uimanager")
local ReaderUI = require("apps/reader/readerui")
local attempts = 0
local function ready()
    attempts = attempts + 1
    local ui = ReaderUI.instance
    if not (ui and ui.document and ui.highlight and ui.rolling) then
        assert(attempts < 100, "Stock document did not open")
        UIManager:scheduleIn(0.25, ready)
        return
    end
    local before = ui.document:getXPointer()
    local found = assert(ui.document:findAllText("jade & blue", false, 0, 4, false, 0))
    assert(#found == 2)
    local first, last = found[1], found[2]
    local stock = ui.document.getHTMLFromXPointers
    local case = "first"
    ui.document.getHTMLFromXPointers = function(doc, a, b, flags, root)
        assert(flags == 0xE830 and root == true)
        local html, css, offsets = stock(doc, a, b, flags, root)
        assert(type(html) == "string")
        assert(html:find("jade &amp; blue", 1, true))
        assert(not html:find("Outside left", 1, true))
        assert(not html:find("ends here", 1, true))
        assert(html:find(case == "first" and "<em" or "<strong", 1, true))
        assert(not html:find(case == "first" and "<strong" or "<em", 1, true))
        io.stdout:write("CASE ", case, "\n", html, "\n")
        return html, css, offsets
    end
    for _, hit in ipairs{first, last} do
        ui.highlight.selected_text = { pos0 = hit.start, pos1 = hit["end"], text = hit.matched_text }
        ui.highlight:viewSelectionHTML()
        local stack = UIManager._window_stack
        local popup = assert(stack[#stack].widget)
        assert(popup ~= ui)
        UIManager:close(popup)
        case = "second"
    end
    assert(ui.document:getXPointer() == before)
    io.stdout:write("Stock selection XPointer clipping, repeated-word provenance and return verified\n")
    io.stdout:flush()
    UIManager:quit(0)
end
local function run()
    local ok, err = xpcall(ready, debug.traceback)
    if not ok then io.stderr:write(err, "\n"); UIManager:quit(2) end
end
UIManager:scheduleIn(0.5, run)
