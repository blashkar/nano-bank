import os, sys
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import state  # noqa: E402


def test_read_jsonl_skips_blank_and_partial_lines():
    text = '{"event": 1}\n\n{"final": 2}\n{bad partial'
    rows = state.read_jsonl(text)
    assert rows == [{"event": 1}, {"final": 2}]


def test_beat_catalog_parses_titles_without_importing(tmp_path):
    drive = tmp_path / "drive.py"
    drive.write_text(
        'BEATS = [\n'
        '  {"title": "Call to order — the agenda", "shows": "x", "message": "m", "thread": "board"},\n'
        '  {"title": "Verified minutes", "shows": "y", "message": "m2", "thread": "board"},\n'
        ']\n')
    cat = state.beat_catalog(str(drive))
    assert [b["title"] for b in cat] == ["Call to order — the agenda", "Verified minutes"]


def test_outcome_chip_marks_lever_fired():
    assert "fired" in state.outcome_chip("acted").lower()
    assert state.outcome_chip("unknown") == ""
