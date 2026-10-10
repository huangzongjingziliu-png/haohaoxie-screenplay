"""生成用于 OCR 测试的样本图片。

使用 PIL 绘制一个含场景标题、人物、对白的简单截图，模仿剧本扫描页。
"""
from PIL import Image, ImageDraw, ImageFont
import sys, os

W, H = 850, 1100
img = Image.new("RGB", (W, H), "white")
draw = ImageDraw.Draw(img)

font_paths = [
    "/System/Library/Fonts/Supplemental/Courier New.ttf",
    "/System/Library/Fonts/Courier.ttc",
    "/System/Library/Fonts/Helvetica.ttc",
]
font_path = None
for p in font_paths:
    try:
        ImageFont.truetype(p, 18)
        font_path = p
        break
    except Exception:
        continue

if not font_path:
    print("no truetype font found", file=sys.stderr)
    sys.exit(1)

font = ImageFont.truetype(font_path, 18)
bold_font = ImageFont.truetype(font_path, 24)

lines = [
    (60, 60, "EXT. CITY STREET - DAY", True),
    (150, 150, "JOHN", False),
    (300, 180, "I can't believe we made it.", False),
    (300, 210, "(whispering)", False),
    (60, 260, "CUT TO:", False),
    (60, 310, "INT. DINER - NIGHT", True),
    (150, 400, "MARIA", False),
    (300, 430, "You're late.", False),
]
for x, y, text, is_bold in lines:
    f = bold_font if is_bold else font
    draw.text((x, y), text, fill="black", font=f)

out = os.path.join(os.path.dirname(os.path.abspath(__file__)), "ocr_sample.png")
img.save(out)
print(f"wrote {out} with {len(lines)} text lines")
