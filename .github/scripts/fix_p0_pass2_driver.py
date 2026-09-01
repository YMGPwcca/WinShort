from pathlib import Path

p = Path('.github/scripts/p0_ux_pass2.py')
s = p.read_text(encoding='utf-8')
old = "insert_marker = '''                ElementId::DesktopStripItem(index) => {\\n'''"
new = "insert_marker = '''                ElementId::DesktopStripItem(index) => {\\n                    controls::draw_desktop_item(\\n'''"
if s.count(old) != 1:
    raise SystemExit(f'pass2 marker patch mismatch: {s.count(old)}')
s = s.replace(old, new)
p.write_text(s, encoding='utf-8', newline='\n')
print('pass2 paint marker scoped')
