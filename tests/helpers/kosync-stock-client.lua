-- Authored loopback interoperability driver; uses the unchanged pinned stock client.
require("setupkoenv")
G_defaults=require("luadefaults"):open()
G_reader_settings=require("luasettings"):open(require("datastorage"):getDataDir().."/settings.reader.lua")
local Device=require("device")
require("document/canvascontext"):init(Device)
package.path=package.path..";plugins/kosync.koplugin/?.lua"
local url=assert(os.getenv("KOSYNC_QA_URL"))
assert(url:match("^http://127%.0%.0%.1:%d+/[a-z/]*$"),"QA accepts authored loopback only")
local client=require("KOSyncClient"):new{custom_url=url,service_spec="plugins/kosync.koplugin/api.json"}
local key=require("ffi/sha2").md5("qa-password")
if os.getenv("KOSYNC_QA_MODE")=="push"then assert(client:register("qa",key))end
assert(client:authorize("qa",key))
local document=assert(os.getenv("KOSYNC_QA_DOCUMENT"))
local pdf_pages
local authored_pdf=os.getenv("KOSYNC_QA_PDF")
if authored_pdf then
 local pdf=assert(require("document/documentregistry"):openDocument(authored_pdf))
 assert(pdf.info.has_pages and pdf.provider=="pdf")
 pdf_pages=pdf:getPageCount();assert(pdf_pages==2)
 assert(require("util").partialMD5(authored_pdf)==document)
 pdf:close()
end
if os.getenv("KOSYNC_QA_MODE")=="push"then
 local completed=false
 client:update_progress("qa",key,document,nil,"2",1,"Stock QA","stock-qa",function(ok,status)assert(ok and status==200);completed=true end)
 assert(completed,"Stock client callback was not completed")
end
local result
client:get_progress("qa",key,document,function(ok,body)assert(ok);result=body end)
assert(result and result.progress)
local hashes={}
local hash_cases=os.getenv("KOSYNC_QA_HASH_CASES")
if hash_cases then
 local file=assert(io.open(hash_cases,"r"));local cases=require("json").decode(file:read("*a"));file:close()
 for _,case in ipairs(cases)do hashes[#hashes+1]={name=case.name,digest=require("util").partialMD5(case.path)}end
end
local out=assert(io.open(assert(os.getenv("KOSYNC_QA_OUTPUT")),"w"));out:write(require("json").encode({progress=result.progress,percentage=result.percentage,device=result.device,device_id=result.device_id,hashes=hashes,pdf_pages=pdf_pages}));out:close()
Device:exit()
