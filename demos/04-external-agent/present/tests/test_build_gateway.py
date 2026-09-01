import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import build_gateway  # noqa: E402


def test_build_inlines_events_into_the_template(tmp_path, monkeypatch):
    template = tmp_path / "gateway.template.html"
    template.write_text("<html><script>\nconst EVENTS = /*__EVENTS__*/ [];\n</script></html>")
    rec_dir = tmp_path / "recordings"
    rec_dir.mkdir()
    (rec_dir / "canonical.json").write_text(json.dumps(
        {"events": [{"kind": "plan", "instruction": "hi"}], "captured_at": "t"}))
    out = tmp_path / "gateway.html"

    monkeypatch.setattr(build_gateway, "TEMPLATE", str(template))
    monkeypatch.setattr(build_gateway, "OUT", str(out))
    monkeypatch.setattr(build_gateway, "REC", str(rec_dir / "canonical.json"))

    result_path = build_gateway.build()
    assert result_path == str(out)
    html = out.read_text()
    assert '"kind": "plan"' in html or "'kind': 'plan'" in html or '"kind":"plan"' in html
    assert "hi" in html


def test_build_writes_empty_array_when_no_recording(tmp_path, monkeypatch):
    template = tmp_path / "gateway.template.html"
    template.write_text("<html><script>\nconst EVENTS = /*__EVENTS__*/ [];\n</script></html>")
    out = tmp_path / "gateway.html"

    monkeypatch.setattr(build_gateway, "TEMPLATE", str(template))
    monkeypatch.setattr(build_gateway, "OUT", str(out))
    monkeypatch.setattr(build_gateway, "REC", str(tmp_path / "recordings" / "canonical.json"))

    build_gateway.build()
    assert "const EVENTS = [];" in out.read_text()
