-- Run from the installed KOReader directory with its LuaJIT and library path.
-- Accept only the filenames and published credentials of the authored test set.
local folder = assert(arg[1], "Pass the authored PDF fixture directory")
os.setlocale("C", "numeric")
require("setupkoenv")
local mupdf = require("ffi/mupdf")
local DrawContext = require("ffi/drawcontext")

local total, accepted = 0, 0
for _, name in ipairs({
    "r2", "r3", "r4", "r5", "r6", "r6-long", "r4-owner-only", "r4-latin", "r6-latin", "plain",
}) do
    local reader_password, owner_password = "survey-entry", "survey-editor"
    if name == "r6-long" then
        reader_password = "estuary" .. string.rep("n", 88) .. "river"
    elseif name == "r4-owner-only" then
        reader_password = ""
    elseif name:find("latin", 1, true) then
        reader_password, owner_password = "été", "côté"
    end
    for slot, input in ipairs({"", "wrong", reader_password, owner_password}) do
        local document = mupdf.openDocument(folder .. "/" .. name .. ".pdf")
        local expects_prompt = name ~= "plain" and reader_password ~= ""
        assert(tostring(document:needsPassword()) == tostring(expects_prompt))
        local expected = name == "plain" or slot >= 3 or (slot == 1 and reader_password == "")
        local opened = tostring(document:authenticatePassword(input)) == "true"
        assert(opened == expected, name .. " credential " .. slot)
        if opened then
            assert(document:getPages() == 3 and #document:getToc() == 3)
            local page = document:openPage(2)
            local words = {}
            for _, line in ipairs(page:getPageText()) do
                for _, word in ipairs(line) do table.insert(words, word.word) end
            end
            assert(table.concat(words, " "):find("Wind survey", 1, true))
            local image = page:draw_new(DrawContext.new(), 320, 480, 0, 0)
            image:free()
            page:close()
            accepted = accepted + 1
        end
        document:close()
        total = total + 1
    end
end
assert(total == 40 and accepted == 23)
print("Authored PDF credentials: " .. total .. " checked, " .. accepted .. " read, " .. total-accepted .. " rejected")
