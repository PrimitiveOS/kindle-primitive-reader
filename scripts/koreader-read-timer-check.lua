-- Run from an installed KOReader with its LuaJIT. Clock values and widgets are
-- replaced in memory. The stock plugin and its callbacks are loaded unchanged.
-- This verifier does not access the display, power manager, books or settings.
require("setupkoenv")
local stock_time = require("ui/time")
assert(stock_time.now == stock_time.monotonic_coarse, "different interval clock")
local now = 1000
local event, scheduled, shown = 0, {}, {}
local manager = {}
function manager:scheduleIn(seconds, cb) scheduled[cb] = now + seconds end
function manager:unschedule(cb) scheduled[cb] = nil end
function manager:broadcastEvent(e) if e.name=="ReadTimerExpired" then event=event+1 end end
function manager:show(widget) table.insert(shown, widget) end
function manager:close(widget) end
function manager:getTopmostVisibleWidget() return nil end
local widget = {new=function(_,v)return v end}
local container = {extend=function(_,m)return m end}
local dispatcher={registerAction=function()end}
for _,p in ipairs({"checkbutton","confirmbox","datetimewidget","infomessage","notification"})do package.loaded["ui/widget/"..p]=widget end
package.loaded["ui/widget/container/widgetcontainer"]=container
package.loaded["ui/uimanager"]=manager
package.loaded["ui/time"]={now=function()return now end, s=function(v)return v end,to_s=function(v)return v end}
package.loaded["ui/event"]={new=function(_,name)return {name=name}end}
package.loaded.dispatcher=dispatcher
package.loaded.gettext=function(v)return v end
package.loaded.logger={dbg=function()end}
package.loaded.datetime={secondsToClockDuration=function(_,s)return tostring(s)end}
package.loaded["ffi/util"]={template=function(str)return str end}
local settings={}
G_reader_settings={readSetting=function(_,name)if name=="readtimer" then return settings end end}
local timer=dofile("plugins/readtimer.koplugin/main.lua")
timer.ui={bookinfo={expandString=function(_,s)return s end},registerPostInitCallback=function()end,menu={registerToMainMenu=function()end}}
timer:init()
settings.remain_time_hours=0
settings.remain_time_minutes=2
timer:onStartTimer()
assert(timer:remaining()==120)
-- Time spent asleep does not advance CLOCK_MONOTONIC. Wake before expiry
-- reschedules only the unconsumed seconds and produces no alert.
now=now+30
timer:onResume()
assert(timer:remaining()==90 and event==0 and scheduled[timer.alarm_callback]==now+90)
-- A deadline that actually passed on the UI clock is delivered exactly once.
now=now+97
timer:onResume()
assert(event==1 and not timer:scheduled() and shown[#shown].ok_text=="Repeat")
shown[#shown].ok_callback()
assert(timer:remaining()==120)
timer:onStopTimer()
assert(not timer:scheduled())
-- Normal expiry auto-repeats from delivery time. The stock onResume handler
-- explicitly clears the new timer after delivery of an already-expired one.
settings.auto_reschedule_interval=true
settings.show_on_expiry="notification"
timer:onStartTimer()
now=now+120
timer.alarm_callback()
assert(event==2 and timer:remaining()==120 and shown[#shown].timeout==5)
now=now+120
timer:onResume()
assert(event==3 and not timer:scheduled())
settings.show_on_expiry="nothing"
local count=#shown
timer:onStartTimer()
now=now+121
timer.alarm_callback()
assert(event==4 and timer:remaining()==120 and #shown==count+1)
-- Changing from the book to another view within the process restores the due time.
timer:onCloseWidget()
assert(not timer:scheduled())
timer:init()
assert(timer:remaining()==120)
timer:onStopTimer()
assert(not timer:scheduled())
print("read-timer-stock callbacks=4 resume_remaining=90 repeat_seconds=120 notification_seconds=5 view_restore=1 monotonic=1")
