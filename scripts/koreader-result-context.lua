-- Compare the title path shown by an installed ReaderSearch result hold. Load
-- the actual ReaderToc methods without initialising a display or user history.
return function(toc, destination, document)
    local input = assert(io.open("frontend/apps/reader/modules/readertoc.lua", "rb"))
    local source = input:read("*a")
    input:close()
    local function part(first, following)
        local start = assert(source:find("function ReaderToc:" .. first .. "(", 1, true))
        local finish = assert(source:find("function ReaderToc:" .. following .. "(", start, true))
        return source:sub(start, finish-1)
    end
    local methods = assert(loadstring("local ReaderToc={}\nlocal logger={dbg=function() end, info=function() end}\n"
        .. part("cleanUpTocTitle", "onSetDimensions")
        .. part("validateAndFixToc", "completeTocWithChapterLengths")
        .. part("getTocIndexByPage", "getTocTitleOfCurrentPage")
        .. "\nreturn ReaderToc"))()
    local instance = setmetatable({
        toc=toc, toc_ticks_ignored_levels={}, ui={document=document},
        fillToc=function() end, getMaxDepth=function(self) return self.toc_depth end,
    }, {__index=methods})
    instance:validateAndFixToc()
    local path = instance:getFullTocTitleByPage(destination)
    local md5 = require("ffi/sha2").md5
    local hashes = {}
    for _, title in ipairs(path) do hashes[#hashes+1] = md5(title) end
    return hashes
end
