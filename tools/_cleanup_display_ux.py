from pathlib import Path

path = Path(__file__).resolve().parents[1] / "src/display/mod.rs"
text = path.read_text(encoding="utf-8")
old = '''    pub const ALL: [Self; 5] = [
        Self::Internal,
        Self::Clone,
        Self::Extend,
        Self::External,
        Self::Custom,
    ];

'''
if text.count(old) != 1:
    raise SystemExit(f"expected one DisplayTopology::ALL block, got {text.count(old)}")
path.write_text(text.replace(old, "", 1), encoding="utf-8")
print("removed unused DisplayTopology::ALL")
