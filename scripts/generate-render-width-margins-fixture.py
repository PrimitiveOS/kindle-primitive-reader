"""Build an EPUB with authored fixed, constrained or intrinsic text box widths."""

from pathlib import Path
import sys
import zipfile

signed = "--signed" in sys.argv[1:]
fit_content = "--fit-content" in sys.argv[1:]
fit_decorated = "--fit-decorated" in sys.argv[1:]
fit_vertical = "--fit-vertical" in sys.argv[1:]
fit_height = "--fit-height" in sys.argv[1:]
name = "render-signed-widths.epub" if signed else "render-width-margins.epub"
if fit_content:
    name = "render-fit-content.epub"
if fit_decorated:
    name = "render-fit-decorated.epub"
if fit_vertical:
    name = "render-fit-vertical.epub"
if fit_height:
    name = "render-fit-height.epub"
fixture = Path(__file__).resolve().parents[1] / "tests/fixtures" / name
paths = [value for value in sys.argv[1:] if value not in {"--signed", "--fit-content", "--fit-decorated", "--fit-vertical", "--fit-height"}]
output = Path(paths[0]) if paths else fixture
container = """<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"""
opf = """<package version="2.0" unique-identifier="id" xmlns="http://www.idpf.org/2007/opf">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">reader-width-margins</dc:identifier>
<dc:title>Width and Margin Soundings</dc:title><dc:language>en</dc:language></metadata>
<manifest><item id="sample" href="sample.xhtml" media-type="application/xhtml+xml" /></manifest>
<spine><itemref idref="sample"/></spine></package>"""
chapter = """<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Width and Margin Soundings</title>
<style type="text/css">
body,p { margin:0; padding:0; text-align:left; text-indent:0 }
.physical { width:320px; margin-left:40px; margin-right:24px; background:#ddd }
.trailing { width:40%; margin-left:auto; margin-right:56px; background:#ccc }
.maximum { width:520px; max-width:380px; min-width:240px; margin:0 auto; background:#bbb }
.minimum { width:300px; max-width:210px; min-width:36%; margin:0 auto; background:#aaa }
.cap { max-width:420px; margin:0 auto; background:#999 }
.overconstraint { width:1000px; margin-left:80px; margin-right:120px; background:#888 }
.direction { width:320px; margin-left:40px; margin-right:24px; text-align:right; background:#777 }
.free { max-width:1200px; margin-left:52px; margin-right:12px; background:#666 }
</style></head><body>
<p>Opening field markers</p>
<p class="physical"><a href="#west">Fixed marker</a> charts the landing at the west pier and the path up the steep bank.</p>
<p class="trailing">Trailing marker</p>
<p class="maximum">Maximum marker</p>
<p class="minimum">Minimum marker</p>
<p class="cap">Auto cap marker</p>
<p class="overconstraint">Overflow marker</p>
<p class="direction" dir="rtl">Direction marker</p>
<p class="free">Natural marker</p>
<p id="west">West bank directions</p>
</body></html>"""

if signed:
    opf = opf.replace("reader-width-margins", "reader-signed-widths")
    chapter = """<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Signed width soundings</title>
<style type="text/css">
body,p { margin:0; padding:0; text-align:left; text-indent:0 }
.left { width:400px; margin-left:-24px; margin-right:12px; background:#ddd }
.right { width:480px; margin-left:auto; margin-right:-24px; text-align:right; background:#ccc }
.wide { width:1300px; margin-left:auto; margin-right:-24px; text-align:right; background:#bbb }
.bleed { width:1240px; margin-left:-74px; margin-right:40px; background:#aaa }
.direction { width:1100px; margin-left:40px; margin-right:-78px; text-align:right; background:#999 }
.cap { max-width:420px; margin-left:-32px; margin-right:auto; background:#888 }
.minimum { min-width:112%; max-width:380px; margin-left:-34px; margin-right:8px; text-align:right; background:#777 }
.relative { width:104%; margin-left:auto; margin-right:-84px; text-align:right; background:#666 }
</style></head><body>
<p>Reading the channel</p>
<p class="left"><a href="#channel">Left beacon</a> marks the entrance to the west channel.</p>
<p class="right">Look for the <a href="#channel">Right beacon</a></p>
<p class="wide">The rope trails toward the <a href="#channel">Reef line</a></p>
<p class="bleed"><a href="#channel">Low headland</a> stays in view beside the bay.</p>
<p class="direction" dir="rtl">Turning basin</p>
<p class="cap">One small lookout</p>
<p class="minimum">The staff rises above the <a href="#channel">Saltbush</a></p>
<p class="relative">Look out beyond the <a href="#channel">Ebb gate</a></p>
<p id="channel">West channel directions</p>
</body></html>"""

if fit_content:
    opf = opf.replace("reader-width-margins", "reader-fit-content").replace(
        "Width and Margin Soundings", "Intrinsic Width Soundings"
    )
    current = "long coast readings proceed along the winding shelf beyond weather gauge " * 5
    chapter = f"""<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Intrinsic Width Soundings</title>
<style type="text/css">
body,p {{ margin:0; padding:0; text-align:left; text-indent:0 }}
.fit {{ width:fit-content }}
</style></head><body>
<p class="fit" style="background:#ddd">Shore marker </p>
<p class="fit" style="background:#d3d3d3;margin:0 auto">Harbor marker </p>
<p class="fit" style="background:#c9c9c9;margin-left:auto;margin-right:56px">Anchor marker </p>
<p class="fit" style="background:#bfbfbf;max-width:150px;margin:0 auto">Beacon marker </p>
<p class="fit" style="background:#b5b5b5;min-width:420px;margin:0 auto">Needle marker </p>
<p class="fit" style="background:#8d8d8d;margin-left:-24px;margin-right:12px"><a href="#channel">Sounding marker</a> </p>
<p class="fit" style="background:#838383;margin-left:20px;margin-right:12px">Current marker {current}</p>
<p class="fit" style="background:#797979;max-width:22%;min-width:30%;margin:0 auto">Ledger marker </p>
<p class="fit" style="background:#6f6f6f;min-width:110%;margin-left:-24px;margin-right:auto">Quartz marker </p>
<p class="fit" style="background:#515151;margin-left:40px;margin-right:24px;text-align:right" dir="rtl">Turn marker </p>
<p id="channel">Channel directions</p>
</body></html>"""

if fit_decorated:
    opf = opf.replace("reader-width-margins", "reader-fit-decorated").replace(
        "Width and Margin Soundings", "Framed water marks"
    )
    wide = "tidal readings pass along the winding shelf beyond the weather gauge " * 4
    chapter = f'''<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Framed water marks</title>
<style>body,p {{margin:0;padding:0;text-align:left;text-indent:0}}
p {{width:fit-content}}
.physical {{padding-left:20px;padding-right:12px;margin:0 auto}}
</style></head><body>
<p class="physical" style="background:#ddd">Compass marker</p>
<p class="physical" style="background:#d3d3d3;max-width:168px">Channel marker</p>
<p class="physical" style="background:#c9c9c9;max-width:168px;box-sizing:border-box">Ripple marker</p>
<p style="background:#bfbfbf;padding-left:5%;padding-right:3%;margin:0 auto">Measure marker</p>
<p style="background:#b5b5b5;margin-left:-24px;margin-right:12px;padding-left:18px;padding-right:9px;border-left:5px solid #333;border-right:7px solid #555"><a href="#shore">Boundary marker</a></p>
<p style="background:#ababab;padding-left:12px;padding-right:8px;min-width:30%;max-width:22%;margin:0 auto">Ledger marker</p>
<p style="background:#a1a1a1;border:4px solid #444;padding-left:8px;padding-right:4px;min-width:400px;box-sizing:border-box;margin-left:auto;margin-right:24px">Beacon marker</p>
<p class="physical" style="background:#979797">Wide marker {wide}</p>
<p id="shore">The shore path</p>
</body></html>'''

if fit_vertical:
    opf = opf.replace("reader-width-margins", "reader-fit-vertical").replace(
        "Width and Margin Soundings", "Between the sea marks"
    )
    passage = "<br/>\n".join(f"Sounding {i:02d}" for i in range(1, 43))
    chapter = f'''<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Between the sea marks</title>
<style type="text/css">
body,p,section {{margin:0;padding:0;text-align:left;text-indent:0}}
.fit {{width:fit-content; margin:0 auto; padding-left:16px; padding-right:20px}}
.passage {{max-width:460px; padding:28px 20px 37px; border:3px solid #444;
    background:#bbb; page-break-before:always; page-break-after:always}}
</style></head><body>
<p class="fit" style="background:#ddd">Base marker</p>
<p class="fit" style="padding-top:26px; padding-bottom:19px; border-top:3px solid #333;
    border-bottom:4px solid #555; background:#ccc"><a href="#shore">Upper ledge</a></p>
<section><p class="fit" style="padding-top:3%; padding-bottom:2%; max-width:440px;
    box-sizing:border-box; background:#aaa">Lower ledge</p></section>
<p class="fit" style="padding-top:.5em; padding-bottom:1em; font-size:125%;
    background:#999">Long measure</p>
<p class="fit" style="margin-left:-14px; margin-right:18px; padding-top:2%;
    padding-bottom:1%; background:#888">Sloping bank</p>
<p class="fit passage">{passage}</p>
<p id="shore">After the passage</p>
</body></html>'''

if fit_height:
    opf = opf.replace("reader-width-margins", "reader-fit-height").replace(
        "Width and Margin Soundings", "Along the tidal inlet"
    )
    chapter = '''<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Along the tidal inlet</title>
<style>
body,p,section {margin:0;padding:0;text-align:left;text-indent:0}
.fit {width:fit-content;margin:0 auto;padding:18px 16px 22px 12px;
    border-top:2px solid #333;border-bottom:3px solid #555}
.first {page-break-before:always}
</style></head><body>
<p class="fit" style="height:180px;background:#ddd"><a href="#route">Content bay</a></p>
<p class="fit" style="height:180px;box-sizing:border-box;background:#ccc">Border bay</p>
<p class="fit" style="height:220px;max-height:100px;background:#bbb">Narrow basin</p>
<p class="fit" style="height:220px;max-height:100px;min-height:155px;background:#aaa">North channel</p>
<p class="fit" style="min-height:136px;background:#999">Quiet water</p>
<p class="fit" style="height:60%;min-height:30%;max-height:70%;background:#888">Current line</p>
<p class="fit" style="max-height:20px;background:#777">Twin banks<br/>Lower bank</p>
<p class="fit first" style="height:3em;min-height:4rem;font-size:125%;background:#ddd">Deep reach</p>
<section style="box-sizing:border-box"><p class="fit" style="height:160px;background:#ccc">Open shore</p></section>
<p class="fit" style="height:360pt;max-height:0;background:#bbb">Tidal fall</p>
<p class="fit" style="height:104.75pt;background:#888">Bright harbor</p>
<p id="route" class="fit first" style="height:2200px;border:3px solid #444;background:#aaa">Far headland</p>
<p>After the headland</p>
<p class="fit first" style="height:2200px;border:3px solid #444;background:#999;page-break-after:always">Last headland</p>
<p>After the last headland</p>
</body></html>'''

with zipfile.ZipFile(output, "w") as book:
    for name, contents in [
        ("mimetype", "application/epub+zip"),
        ("META-INF/container.xml", container),
        ("content.opf", opf),
        ("sample.xhtml", chapter),
    ]:
        entry = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
        entry.compress_type = zipfile.ZIP_STORED if name == "mimetype" else zipfile.ZIP_DEFLATED
        book.writestr(entry, contents)
