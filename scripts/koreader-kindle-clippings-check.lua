-- Execute the pinned Kindle annotation parser and text exporter on authored data.
-- Usage: TZ=UTC lua scripts/koreader-kindle-clippings-check.lua <exporter directory>
local source = assert(arg[1], "Pass a directory containing pinned clip.lua and my_clippings.lua")
local original_open = io.open

package.preload["ui/widget/booklist"] = function() return {} end
package.preload["document/documentregistry"] = function() return {} end
package.preload["ffi/sha2"] = function() return {} end
package.preload["gettext"] = function() return function(value) return value end end
package.preload["ffi/util"] = function()
    return { template = function(format, ...)
        local values = { ... }
        return (format:gsub("%%(%d)", function(index) return tostring(values[tonumber(index)]) end))
    end }
end
package.preload["base"] = function()
    return { new = function(_, target)
        target.getFilePath = function() return "authored-export.txt" end
        return target
    end }
end

local fixture = table.concat({
    "Harbor Notebook (Linh Nguyen)",
    "- Your Highlight on page 42 | Location 70-75 | Added on 2024-02-29 10:15:02",
    "",
    "The lantern marks the channel.",
    "==========",
    "Harbor Notebook (Linh Nguyen)",
    "- Your Note on Location 94 | Added on 2024-02-29 10:20:00",
    "",
    "Compare the tide table.",
    "==========",
    "",
}, "\n")

io.open = function(path, mode)
    assert(path == "/mnt/us/documents/My Clippings.txt" and mode == "r")
    local scratch = assert(io.tmpfile())
    assert(scratch:write(fixture))
    assert(scratch:seek("set"))
    return scratch
end
local parser = dofile(source .. "/clip.lua"):new{}
local parsed = parser:parseMyClippings()
io.open = original_open
local book = assert(parsed["Harbor Notebook"])
assert(book.author == "Linh Nguyen")
assert(#book == 2)
assert(book[1][1].sort == "highlight" and book[1][1].page == 42)
assert(book[1][1].time == os.time{year=2024, month=2, day=29, hour=10, min=15, sec=2})
assert(book[1][1].text == "The lantern marks the channel.")
assert(book[2][1].sort == "note" and book[2][1].page == "94")
assert(book[2][1].text == "Compare the tide table.")

local written = {}
io.open = function(path, mode)
    assert(path == "authored-export.txt" and mode == "a")
    return {
        write = function(_, content) table.insert(written, content) end,
        close = function() end,
    }
end
local exporter = dofile(source .. "/my_clippings.lua")
book[1][1].note = "Compare the tide table."
book[2] = nil
assert(exporter:export{book})
io.open = original_open
assert(written[1] == table.concat({
    "Harbor Notebook (Linh Nguyen)",
    "- Your highlight on page 42 | Added on Thursday, February 29, 2024 10:15:02 AM",
    "",
    "The lantern marks the channel.",
    "==========",
    "Harbor Notebook (Linh Nguyen)",
    "- Your note on page 42 | Added on Thursday, February 29, 2024 10:15:02 AM",
    "",
    "Compare the tide table.",
    "==========",
    "",
}, "\n"))
print("Stock Kindle highlights, locations, notes and clippings text verified")
