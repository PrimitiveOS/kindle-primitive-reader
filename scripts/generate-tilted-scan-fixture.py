import sys, zlib, math, re
src, out, deg = sys.argv[1], sys.argv[2], float(sys.argv[3])
# Read the grayscale page from the speckled PDF's Flate image (same generator as make_speck.py).
data = open(src, "rb").read()
m = re.search(rb"/Width (\d+) /Height (\d+).*?stream\n(.*?)\nendstream", data, re.S)
W, H = int(m.group(1)), int(m.group(2))
raw = zlib.decompress(m.group(3))
t = math.radians(deg); c, s = math.cos(t), math.sin(t)
cx, cy = W / 2, H / 2
img = bytearray([255]) * (W * H)
for y in range(H):
    for x in range(W):
        dx, dy = x - cx, y - cy
        sx = int(round(c * dx + s * dy + cx)); sy = int(round(-s * dx + c * dy + cy))
        if 0 <= sx < W and 0 <= sy < H:
            img[y * W + x] = raw[sy * W + sx]
comp = zlib.compress(bytes(img))
objs = [b"<< /Type /Catalog /Pages 2 0 R >>", b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 %d %d] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>" % (W, H),
    b"<< /Type /XObject /Subtype /Image /Width %d /Height %d /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Length %d >>\nstream\n" % (W, H, len(comp)) + comp + b"\nendstream"]
content = b"q %d 0 0 %d 0 0 cm /Im0 Do Q" % (W, H)
objs.append(b"<< /Length %d >>\nstream\n" % len(content) + content + b"\nendstream")
pdf = bytearray(b"%PDF-1.4\n"); offs = []
for i, body in enumerate(objs, 1):
    offs.append(len(pdf)); pdf += b"%d 0 obj\n" % i + body + b"\nendobj\n"
xref = len(pdf)
pdf += b"xref\n0 %d\n0000000000 65535 f \n" % (len(objs) + 1)
for o in offs: pdf += b"%010d 00000 n \n" % o
pdf += b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % (len(objs) + 1, xref)
open(out, "wb").write(pdf); print(out, deg)
