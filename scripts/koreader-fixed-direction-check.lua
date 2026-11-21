-- Run from the installed KOReader directory. Only reads the book and its bundled
-- modules; avoids creating a display, a cache or normal reading history.
local path, query, current = arg[1], arg[2], tonumber(arg[3])
assert(path and path:sub(1,1) == "/" and query and current and current >= 1)
require("setupkoenv")
local input = assert(io.open("frontend/document/koptinterface.lua", "rb"))
local source = input:read("*a")
input:close()
local start = assert(source:find("function KoptInterface:nativeToPageRectTransform(",1,true))
local finish = assert(source:find("function KoptInterface:findAllText(",start,true))
local factory = assert(loadstring("local util, Utf8Proc = ...\nlocal logger={dbg=function() end}\nlocal KoptInterface={}\n"
    .. source:sub(start,finish-1) .. "\nreturn KoptInterface"))
local kopt = factory(require("util"), require("ffi/utf8proc"))
local pdf = require("ffi/mupdf").openDocument(path)
local doc = {configurable={text_wrap=0}}
function doc:getPageCount() return pdf:getPages() end
function doc:getPageTextBoxes(n)
    local p = pdf:openPage(n)
    local lines = p:getPageText()
    p:close()
    return lines
end
if arg[4] == "--word-edges" then
    local counts = {}
    for _, pattern in ipairs({query, " " .. query, query .. " ", " " .. query .. " "}) do
        local matches = kopt:findAllMatches(doc, pattern, true, current)
        counts[#counts+1] = matches and #matches or 0
    end
    pdf:close()
    print(require("json").encode({page=current, word_boxes=counts}))
    return
end
local pos = current
local pages, boxes = {}, {}
for _, step in ipairs({{0,0},{1,0},{1,1},{1,1}}) do
    local hit = assert(kopt:findText(doc,query,step[1],step[2],true,pos))
    pos = assert(hit.page)
    pages[#pages+1] = pos
    boxes[#boxes+1] = #hit
end
pdf:close()
print(require("json").encode({visited=pages,word_boxes=boxes}))
