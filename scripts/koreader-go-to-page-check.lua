-- Run with LuaJIT and the pinned KOReader source directory as the sole argument.
local root = assert(arg[1], "Pass the pinned KOReader source directory")
local modules = root .. "/frontend/apps/reader/modules/"
local function mock(name, value)
    package.preload[name] = function() return value end
end
mock("gettext", function(value) return value end)
mock("ffi/util", {template = function(value) return value end})
mock("device", {screen = {}})
mock("ui/event", {new = function(_, name, value) return {name, value} end})
mock("ui/widget/container/widgetcontainer", {extend = function(_, widget) return widget end})
mock("ui/uimanager", {close = function(_, dialog) dialog.closed = true end})
for _, name in ipairs{"ui/geometry", "ui/widget/inputdialog", "ui/widget/skimtowidget", "util"} do
    mock(name, {})
end

-- Load only the handlers under comparison. The production UI and event loop
-- have no role in mapping these authored destination references.
local function stock_handler(path, class, method, environment)
    local f = assert(io.open(path))
    local source = f:read("*a")
    f:close()
    local start = assert(source:find("function " .. class .. ":" .. method .. "(", 1, true))
    local close = assert(source:find("\nend\n", start, true))
    local chunk = assert(loadstring(source:sub(start, close + 3) .. "\nreturn " .. class .. "." .. method))
    return setfenv(chunk, setmetatable(environment, {__index = _G}))()
end
local paging = {}
local paging_environment = {ReaderPaging = paging, logger = {dbg = function() end}}
local fixed_relative = stock_handler(modules .. "readerpaging.lua", "ReaderPaging", "onGotoRelativePage", paging_environment)
local fixed_percent = stock_handler(modules .. "readerpaging.lua", "ReaderPaging", "onGotoPercent", paging_environment)
local rolling = {}
local reflow_relative = stock_handler(modules .. "readerrolling.lua", "ReaderRolling", "onGotoRelativePage", {ReaderRolling = rolling})
local reflow_percent = stock_handler(modules .. "readerrolling.lua", "ReaderRolling", "_gotoPercent", {ReaderRolling = rolling})

local goto_stock = dofile(modules .. "readergoto.lua")
local events, history
local authored_flows = { [0] = {1, 4}, {3, 2}, {7, 2} }
local document = {
    flows = authored_flows,
    getPageCount = function() return 8 end,
    getXPointer = function() return "authored-source" end,
    hasHiddenFlows = function() return true end,
    getTotalPagesInFlow = function(self, flow) return self.flows[flow][2] end,
    getFirstPageInFlow = function(self, flow) return self.flows[flow][1] end,
    getLastLinearPage = function() return 6 end,
    getPageFlow = function(_, page)
        if page == 3 or page == 4 then return 1 end
        if page == 7 or page == 8 then return 2 end
        if page >= 1 and page <= 8 then return 0 end
        return -1
    end,
}
local cre_path = root .. "/frontend/document/credocument.lua"
local cre = {CreDocument = {}, Document = {}}
document.getNextPage = stock_handler(cre_path, "CreDocument", "getNextPage", cre)
document.getPrevPage = stock_handler(cre_path, "CreDocument", "getPrevPage", cre)
local ui = {
    document = document,
    link = {addCurrentLocationToStack = function() history = history + 1 end},
    handleEvent = function(_, event) events[#events + 1] = event end,
}
local editor = setmetatable({ document = document, ui = ui }, {__index = goto_stock})
local function submit(input, percent)
    history, events = 0, {}
    editor.goto_dialog = {
        getInputText = function() return input end,
        getInputValue = function() return tonumber(input) end,
    }
    if percent then editor:gotoPercent() else editor:gotoPage() end
    return events[1]
end
local event = assert(submit("4"))
assert(event[1] == "GotoPage" and event[2] == 4 and history == 1 and editor.goto_dialog.closed)
event = assert(submit("+3"))
assert(event[1] == "GotoRelativePage" and event[2] == 3 and history == 1)
local fixed = {ui = ui, current_page = 1, number_of_pages = 8, _gotoPage = function(self, page) self.captured = page end}
fixed_relative(fixed, event[2])
assert(fixed.captured == 6)
fixed_relative(fixed, -999999)
assert(fixed.captured == 1)
local reflow = {view = {view_mode = "page"}, ui = ui, current_page = 2, _gotoPage = fixed._gotoPage}
reflow_relative(reflow, event[2])
assert(reflow.captured == 5 and reflow.xpointer == "authored-source")
assert(submit("[4]")[2] == 6 and history == 0)
assert(submit("[2]1")[2] == 4 and history == 0)
assert(submit("[3]2") == nil and not editor.goto_dialog.closed)
event = assert(submit("25", true))
assert(event[1] == "GotoPercent" and event[2] == 25 and history == 1)
fixed_percent(fixed, event[2]); assert(fixed.captured == 2)
reflow_percent(reflow, event[2]); assert(reflow.captured == 2)
ui.pagemap = {
    wantsPageLabels = function() return true end,
    cleanPageLabel = function(_, text) return text:lower() end,
    getPageLabelProps = function(_, label) return nil, label == "iv" and 4 or nil end,
}
assert(submit("IV")[2] == 4 and history == 0)
assert(submit("+3") == nil)
print("Stock numeric, relative, printed-label, percentage and hidden-flow dispatch verified")
