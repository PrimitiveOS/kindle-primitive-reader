-- Evaluate the installed ReaderView granularity on an in-memory 64x40 BB8.
-- The extracted drawing function is unchanged. No UI, book or framebuffer is opened.
require("setupkoenv")
local BB = require("ffi/blitbuffer")
local file = assert(io.open("frontend/apps/reader/modules/readerview.lua", "r"))
local module=file:read("*a"); file:close()
local code = assert(module:match("(function ReaderView:drawHighlightRect%b()%s*.-)\nend\n")) .. "\nend"
local env={ReaderView={}, Blitbuffer=BB, G_reader_settings={readSetting=function()return nil end},
           Size={line={medium=3,thick=4}}, Screen={night_mode=false}}
setmetatable(env,{__index=_G})
local func=assert(loadstring(code)); setfenv(func,env); func()
local tones={0,51,102,153,204,255}
for _,fixed in ipairs({false,true}) do
    for _,style in ipairs({"lighten","underscore","strikeout","invert"}) do
        local bb=BB.new(64,40,BB.TYPE_BB8)
        for y=0,39 do
            for x=0,63 do bb:setPixel(x,y,BB.Color8(tones[(x%6)+1])) end
        end
        local view={highlight={lighten_factor=0.2},ui={paging=fixed}}
        env.ReaderView.drawHighlightRect(view,bb,0,0,{x=6,y=5,w=40,h=14},style,nil,nil)
        local result={}
        for y=4,23 do
            local line={}
            for x=6,11 do table.insert(line,bb:getPixel(x,y):getColor8().a) end
            table.insert(result,table.concat(line,","))
        end
        print("highlight-rows "..(fixed and "fixed"or"reflow").." "..style.." "..table.concat(result,";"))
        bb:free()
    end
end
