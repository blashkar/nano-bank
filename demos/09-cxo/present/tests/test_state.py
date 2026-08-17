import os
import sys
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))
from state import (read_jsonl, save_recording, load_recording,  # noqa: E402
                   latest_recording, outcome_style, beat_catalog)
from scorecard import parse_scorecard  # noqa: E402


def test_read_jsonl_skips_partial_trailing_line():
    text = '{"beat":1}\n{"beat":2}\n{"beat":3'
    assert [b["beat"] for b in read_jsonl(text)] == [1, 2]


def test_recording_round_trip_with_scorecard(tmp_path):
    p = save_recording(str(tmp_path), [{"beat": 1}], {"nps": {"score": 17}})
    rec = load_recording(p)
    assert rec["beats"] == [{"beat": 1}]
    assert rec["scorecard"]["nps"]["score"] == 17


def test_latest_recording_picks_newest(tmp_path):
    a = save_recording(str(tmp_path), [{"beat": 1}])
    import time; time.sleep(0.01)
    b = save_recording(str(tmp_path), [{"beat": 2}])
    assert latest_recording(str(tmp_path)) == b and a != b


def test_outcome_style_labels():
    assert outcome_style("read_only")[0] == "READ-ONLY"
    assert outcome_style("deferred")[0] == "DEFERRED"


def test_beat_catalog_parses_the_cxo_driver():
    drive = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", "drive.py"))
    cat = beat_catalog(drive)
    assert len(cat) >= 6
    assert cat[0]["beat"] == 1 and cat[0]["title"]
    assert any("Survey" in b["title"] for b in cat)


def test_parse_scorecard_takes_last_json_line():
    assert parse_scorecard('noise\n{"nps": {"score": 5}}') == {"nps": {"score": 5}}
    assert parse_scorecard("") == {}
    assert parse_scorecard("not json") == {}
