-- Run from the installed KOReader directory. Only reads the book and its bundled
-- modules; avoids creating a display, a cache or normal reading history.
local path, query, current = arg[1], arg[2], tonumber(arg[3])
assert(path and path:sub(1,1) == "/" and query and current and current >= 1)
require("setupkoenv")
local input = assert(io.open("frontend/document/koptinterface.lua", "rb"))
local source = input:read("*a")
input:close()
local start = assert(source:find("function KoptInterface:nativeToPageRectTransform(",1,true))
local finish = assert(source:find("function KoptInterface:logReflowDuration(",start,true))
local context_start = assert(source:find("local function get_prev_text(", 1, true))
local context_end = assert(source:find("function KoptInterface:getSelectedWordContext(", context_start, true))
local factory = assert(loadstring("local util, Utf8Proc = ...\nlocal logger={dbg=function() end}\nlocal KoptInterface={}\n"
    .. source:sub(context_start,context_end-1) .. source:sub(start,finish-1) .. "\nreturn KoptInterface"))
local kopt = factory(require("util"), require("ffi/utf8proc"))
local pdf = require("ffi/mupdf").openDocument(path)
local doc = {configurable={text_wrap=0}}
local page_sizes = {}
function doc:getPageFlow() return 0 end -- Base Document flow for a fixed PDF.
function doc:getPageCount() return pdf:getPages() end
function doc:getPageTextBoxes(n)
    local p = pdf:openPage(n)
    if arg[4] == "--find-all" then
        page_sizes[n] = {p:getSize(require("ffi/drawcontext").new())}
    end
    local lines = p:getPageText()
    p:close()
    return lines
end
if arg[4] == "--find-all" then
    local results = kopt:findAllText(doc, query, true, 5, 5000) or {}
    local counts, visible = {}, {}
    local last, total_visible = 0, 0
    local function visible_box(item, page)
        local size = assert(page_sizes[page])
        for _, b in ipairs(item.boxes) do
            if b.x < size[1] and b.y < size[2] and b.x + b.w > 0 and b.y + b.h > 0 then
                return true
            end
        end
        return false
    end
    for _, item in ipairs(results) do
        assert(item.start >= last and #item.boxes > 0)
        assert(item.matched_text and #item.matched_text > 0)
        last = item.start
        counts[last] = (counts[last] or 0) + 1
        if visible_box(item, last) then
            visible[last] = (visible[last] or 0) + 1
            total_visible = total_visible + 1
        end
    end
    local capped = #results == 5000
    results = nil
    -- The unmodified stock result above includes word boxes outside the crop.
    -- If its last page was truncated, extend the visible comparison until it
    -- has the same number of drawable targets as Reader.
    if capped and total_visible < 5000 then
        for page = last, doc:getPageCount() do
            local one = {getPageCount=function() return 1 end,
                getPageTextBoxes=function() return doc:getPageTextBoxes(page) end}
            local entries = kopt:findAllText(one, query, true, 0, 5000) or {}
            for i = 1 + (counts[page] or 0), #entries do
                if visible_box(entries[i], page) then
                    visible[page] = (visible[page] or 0) + 1
                    total_visible = total_visible + 1
                    if total_visible == 5000 then break end
                end
            end
            if total_visible == 5000 then break end
        end
    end
    for page = 1, doc:getPageCount() do
        counts[page] = counts[page] or 0
        visible[page] = visible[page] or 0
    end
    pdf:close()
    print("fixed-all " .. require("json").encode({stock=counts, visible=visible}))
    return
end
if arg[4] == "--context" then
    local results = assert(kopt:findAllText(doc, query, true, 5, current))
    local hit = assert(results[current])
    local context = dofile(assert(arg[0]:match("^(.*/)")) .. "koreader-result-context.lua")
    local path = context(pdf:getToc(), hit.start, doc)
    pdf:close()
    print("result-context " .. require("json").encode({page=hit.start, labels=path, emphasized_word=require("ffi/sha2").md5(hit.matched_text)}))
    return
end
if arg[4] == "--unicode" then
    local json = require("json")
    local file = assert(io.open(query, "rb"))
    local cases = json.decode(file:read("*a"))
    file:close()
    local result = {}
    for _, case in ipairs(cases) do
        local page = pdf:openPage(case.page)
        local dc = require("ffi/drawcontext").new()
        local w, h = page:getSize(dc)
        page:close()
        for _, sensitive in ipairs({false,true}) do
            local hits = kopt:findAllMatches(doc, case.query, not sensitive, case.page) or {}
            local rects = {}
            for _, box in ipairs(hits) do
                rects[#rects+1] = {box.x/w, box.y/h, (box.x+box.w)/w, (box.y+box.h)/h}
            end
            result[#result+1] = {page=case.page, case=sensitive, boxes=rects}
        end
    end
    pdf:close()
    print("unicode-boxes " .. json.encode(result))
    return
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
if arg[4] == "--endpoints" then
    local function from_edge(reverse, page)
        return kopt:findText(doc, query, -1, reverse, true, page)
    end
    local tail = assert(from_edge(1, current))
    assert(not from_edge(1, tail.page))
    local head = assert(from_edge(0, tail.page))
    assert(not from_edge(0, head.page))
    local last = assert(from_edge(1, head.page))
    pdf:close()
    print(require("json").encode({
        pages={current,tail.page,head.page,last.page},
        boxes={#tail,#head,#last},
        outside_empty=true,
    }))
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
