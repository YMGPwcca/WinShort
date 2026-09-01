from pathlib import Path

p = Path('.github/scripts/p0_ux_pass2.py')
s = p.read_text(encoding='utf-8')

replacements = [
    (
        "insert_marker = '''                ElementId::DesktopStripItem(index) => {\\n'''",
        "insert_marker = '''                ElementId::DesktopStripItem(index) => {\\n                    controls::draw_desktop_item(\\n'''",
        'paint marker',
    ),
    (
        '''insert_before(\n    "src/ui/control_center.rs",\n    ''' + "'''            ElementId::MicHotkey\\n            | ElementId::OutputHotkey\\n'''" + ''',\n    ''' ,
        '''insert_before(\n    "src/ui/control_center.rs",\n    ''' + "'''            ElementId::MicHotkey\\n            | ElementId::OutputHotkey\\n            | ElementId::ForegroundHotkey\\n            | ElementId::CycleInputHotkey\\n            | ElementId::CycleOutputHotkey\\n            | ElementId::ForegroundVolumeUpHotkey\\n            | ElementId::ForegroundVolumeDownHotkey\\n            | ElementId::PreviousDesktopHotkey\\n            | ElementId::AssignScratchpadHotkey\\n            | ElementId::ToggleScratchpadHotkey\\n            | ElementId::DisplayProfileHotkey => {\\n                self.recording = Some(id);\\n'''" + ''',\n    ''',
        'activation marker',
    ),
]

for old, new, label in replacements:
    if s.count(old) != 1:
        raise SystemExit(f'pass2 {label} patch mismatch: {s.count(old)}')
    s = s.replace(old, new)

p.write_text(s, encoding='utf-8', newline='\n')
print('pass2 paint and activation markers scoped')
