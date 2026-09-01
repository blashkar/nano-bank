import os, sys
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import state  # noqa: E402


def test_read_jsonl_skips_blank_and_partial_lines():
    text = '{"kind": "plan"}\n\n{"kind": "result"}\n{bad partial'
    rows = state.read_jsonl(text)
    assert rows == [{"kind": "plan"}, {"kind": "result"}]


def test_save_and_load_recording_roundtrip(tmp_path):
    events = [{"kind": "plan", "instruction": "do the thing"}]
    path = state.save_recording(str(tmp_path), events)
    assert state.latest_recording(str(tmp_path)) == path
    loaded = state.load_recording(path)
    assert loaded["events"] == events
    assert "captured_at" in loaded


def test_latest_recording_none_when_empty(tmp_path):
    assert state.latest_recording(str(tmp_path)) is None


def test_decision_style_known_and_unknown():
    label, color = state.decision_style("allow")
    assert "ALLOW" in label and color.startswith("#")
    label2, _ = state.decision_style("weird")
    assert label2 == "WEIRD"
