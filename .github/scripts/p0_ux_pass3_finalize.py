from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    (ROOT / path).write_text(text, encoding="utf-8", newline="\n")


def replace(path: str, old: str, new: str, count: int = 1) -> None:
    text = read(path)
    found = text.count(old)
    if found != count:
        raise SystemExit(f"{path}: expected {count} matches, found {found}: {old[:120]!r}")
    write(path, text.replace(old, new))


# Microphone/output state events no longer drive WinShort OSD policy. Their
# origin therefore has no consumer and should not be kept as dead metadata.
replace(
    "src/event.rs",
    '''    MicrophoneStateChanged {\n        state: AudioState,\n        origin: AudioEventOrigin,\n    },\n    OutputStateChanged {\n        state: OutputState,\n        origin: AudioEventOrigin,\n    },\n''',
    '''    MicrophoneStateChanged {\n        state: AudioState,\n    },\n    OutputStateChanged {\n        state: OutputState,\n    },\n''',
)

# The audio worker may still carry an origin internally for rebuild/coalescing
# decisions, but published endpoint state is now just state.
replace(
    "src/audio/controller.rs",
    "                self.post(AppEvent::MicrophoneStateChanged { state, origin });",
    "                self.post(AppEvent::MicrophoneStateChanged { state });",
)
replace(
    "src/audio/controller.rs",
    "                self.post(AppEvent::OutputStateChanged { state, origin });",
    "                self.post(AppEvent::OutputStateChanged { state });",
)

# Preserve the existing publish call shape because rebuild_all still uses the
# origin before publishing (for its own external/default-change bookkeeping).
replace(
    "src/audio/controller.rs",
    "    fn publish(&self, flow: EndpointFlow, origin: crate::event::AudioEventOrigin) {",
    "    fn publish(&self, flow: EndpointFlow, _origin: crate::event::AudioEventOrigin) {",
)

# Degraded-startup state publication follows the same lean event contract.
replace(
    "src/app.rs",
    '''                    self.route_event(AppEvent::MicrophoneStateChanged {\n                        state: crate::audio::AudioState::Unavailable { reason },\n                        origin: AudioEventOrigin::WinShortAction(request_id),\n                    });\n''',
    '''                    self.route_event(AppEvent::MicrophoneStateChanged {\n                        state: crate::audio::AudioState::Unavailable { reason },\n                    });\n''',
)
replace(
    "src/app.rs",
    '''                    self.route_event(AppEvent::OutputStateChanged {\n                        state: crate::audio::OutputState::Unavailable { reason },\n                        origin: AudioEventOrigin::WinShortAction(request_id),\n                    });\n''',
    '''                    self.route_event(AppEvent::OutputStateChanged {\n                        state: crate::audio::OutputState::Unavailable { reason },\n                    });\n''',
)

# Pass 3 has already removed endpoint OSD behavior from the consumers.
replace(
    "src/app.rs",
    "            AppEvent::MicrophoneStateChanged { state, origin: _ } => {",
    "            AppEvent::MicrophoneStateChanged { state } => {",
)
replace(
    "src/app.rs",
    "            AppEvent::OutputStateChanged { state, origin: _ } => {",
    "            AppEvent::OutputStateChanged { state } => {",
)

print("P0 UX pass 3 event cleanup applied")
