-- Read-only nonlinear-flow comparison using the installed engine and navigation functions.
-- Run in the KOReader directory with scratch and a local EPUB as arguments.
local scratch, file = assert(arg[1]), assert(arg[2])
assert(scratch:sub(1, 1) == "/" and not scratch:find("/%.%./"))
assert(scratch ~= "/mnt/us/documents" and scratch:sub(1, 18) ~= "/mnt/us/documents/")
require("setupkoenv")
local lfs = require("libs/libkoreader-lfs")
assert(lfs.attributes(scratch, "mode") == "directory")
assert(lfs.symlinkattributes(file, "mode") == "file")
local function source(path)
    local input = assert(io.open(path, "rb"))
    local text = input:read("*a")
    input:close()
    return text
end
local function method(text, class, name, target, base)
    local start = assert(text:find("function " .. class .. ":" .. name .. "(", 1, true))
    local finish = text:find("\nfunction ", start + 1, true) or (#text + 1)
    local code = text:sub(start, finish - 1)
    -- Evaluate the installed method unchanged, supplying only its class tables.
    assert(loadstring("return function(" .. class .. ", Document)\n" .. code .. "\nend"))()(target, base)
end
local base, methods = {}, {}
local doc_source = source("frontend/document/document.lua")
method(doc_source, "Document", "getNextPage", base, base)
method(doc_source, "Document", "getPrevPage", base, base)
local cre_source = source("frontend/document/credocument.lua")
for _, name in ipairs({"hasHiddenFlows", "hasNonLinearFlows", "getNextPage", "getPrevPage", "getPageFlow", "getPageCount", "getLastLinearPage", "getFirstPageInFlow", "getTotalPagesInFlow", "cacheFlows"}) do
    method(cre_source, "CreDocument", name, methods, base)
end
local cre = require("libs/libkoreader-cre")
cre.initCache(scratch, 0, true, 1)
cre.registerFont("/mnt/us/launcher/fonts/NotoSans-Regular.ttf")
local view = cre.newDocView(1264, 1606, 1)
view:setStyleSheet("./data/epub.css")
view:setFontFace("Noto Sans")
view:setFontSize(42)
view:setDefaultInterlineSpace(135)
view:setPageMargins(54, 54, 54, 54)
view:setVisiblePageCount(1, false)
view:setHeaderInfo(0)
view:setIntProperty("crengine.doc.nonlinear.pagebreak.force", 1)
assert(view:loadDocument(file, false))
view:renderDocument()
local doc = setmetatable({
    _document = view, hide_nonlinear_flows = true,
    _view_mode = 1, PAGE_VIEW_MODE = 1,
    getVisiblePageCount = function() return 1 end,
}, {__index = methods})
doc.info = {number_of_pages = view:getPages()}
doc:cacheFlows()
assert(doc:hasHiddenFlows())
local count, checked, nonlinear_pages = view:getPages(), 0, 0
for page = 1, count do
    local flow = doc:getPageFlow(page)
    if flow > 0 then nonlinear_pages = nonlinear_pages + 1 end
    for _, direction in ipairs({-1, 1}) do
        local wanted = 0
        local candidate = page + direction
        while candidate >= 1 and candidate <= count do
            local next_flow = doc:getPageFlow(candidate)
            if next_flow == 0 or next_flow == flow then wanted = candidate; break end
            candidate = candidate + direction
        end
        local actual = direction > 0 and doc:getNextPage(page) or doc:getPrevPage(page)
        assert(actual == wanted)
        checked = checked + 1
    end
end
local first, last = doc:getNextPage(0), doc:getPrevPage(0)
assert(first > 1 and doc:getPageFlow(first) == 0 and doc:getPageFlow(last) == 0)
local flows = #doc.flows
view:close()
print(string.format('{"pages":%d,"nonlinear_pages":%d,"nonlinear_flows":%d,"first_linear_page":%d,"last_linear_page":%d,"navigation_checks":%d}', count, nonlinear_pages, flows, first, last, checked))
