#!/usr/bin/env python3
"""QA probe: Hebrew meeting summary through the `llama-helper` sidecar.

Starts llama-helper (JSON lines over stdin/stdout), asks a local GGUF model to
summarize qa/hebrew_meeting.txt in Hebrew, measures the result and appends a
markdown section to a report file.

Exit codes: 0 = PASS, 1 = FAIL, 2 = protocol/timeout/launch error.
Example:
  summary_probe.py --helper target/release/llama-helper --model /m/gemma.gguf \
      --transcript qa/hebrew_meeting.txt --report qa/report.md --label "Gemma 4 E4B"
"""
import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

SYSTEM_PROMPT = (
    "**Write the summary/report in Hebrew. Keep names of people, companies, products, "
    "code identifiers and URLs exactly as they appear in the transcript; prose in any "
    "other language is invalid.**\n"
    "You are an assistant that writes concise, well-structured meeting summaries in Markdown."
)
USER_PREFIX = (
    "Summarize this meeting transcript. Use these Markdown sections, with these exact "
    "Hebrew headings: ## סיכום (2-3 sentences), ## נקודות עיקריות (bullets), "
    "## החלטות (bullets), ## משימות (bullets: owner, task, deadline).\n\nTranscript:\n"
)
HEADINGS = ["סיכום", "נקודות עיקריות", "החלטות", "משימות"]
FACTS = ["יעל", "דנה", "רון", "ארבעים דולר", "יום חמישי", "טורבו"]
MARKERS = ["<think>", "</think>", "<|channel|>"]
PING_TIMEOUT = 60
GEN_TIMEOUT = 1800


class ProbeError(Exception):
    """Protocol error, helper crash or timeout."""


class Helper:
    """Line-oriented JSON client for the llama-helper process."""

    def __init__(self, path, stderr_file):
        env = dict(os.environ, LLAMA_IDLE_TIMEOUT="7200")  # keep the model loaded
        self.proc = subprocess.Popen([path], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=stderr_file, env=env)
        self.fd = self.proc.stdout.fileno()
        os.set_blocking(self.fd, False)
        self.buf = b""

    def send(self, obj):
        try:
            self.proc.stdin.write((json.dumps(obj) + "\n").encode("utf-8"))
            self.proc.stdin.flush()
        except OSError as e:
            raise ProbeError(f"cannot write to helper: {e}")

    def recv(self, timeout):
        """Next JSON object from stdout; non-JSON lines are ignored."""
        deadline = time.monotonic() + timeout
        while True:
            while b"\n" in self.buf:
                line, self.buf = self.buf.split(b"\n", 1)
                line = line.strip()
                if line.startswith(b"{"):
                    try:
                        return json.loads(line.decode("utf-8", "replace"))
                    except ValueError:
                        pass
            try:
                chunk = os.read(self.fd, 65536)
                if not chunk:
                    raise ProbeError(f"helper closed stdout (exit code {self.proc.poll()})")
                self.buf += chunk
                continue
            except BlockingIOError:
                pass
            if self.proc.poll() is not None and not self.buf:
                raise ProbeError(f"helper exited with code {self.proc.returncode}")
            if time.monotonic() > deadline:
                raise ProbeError(f"timeout after {timeout}s waiting for helper")
            time.sleep(0.05)

    def request(self, obj, timeout):
        self.send(obj)
        return self.recv(timeout)

    def close(self):
        try:
            if self.proc.poll() is None:
                self.send({"type": "shutdown"})
                self.recv(10)
        except ProbeError:
            pass
        try:
            self.proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait()


def generate(helper, args, transcript):
    """One generate request; returns (text, seconds)."""
    req = {
        "type": "generate",
        "messages": [{"role": "system", "content": SYSTEM_PROMPT},
                     {"role": "user", "content": USER_PREFIX + transcript}],
        "max_tokens": args.max_tokens, "context_size": 8192,
        "model_path": str(Path(args.model).resolve()),
        "temperature": 0.3, "top_k": 40, "top_p": 0.9, "repeat_penalty": 1.05,
        "stop_tokens": ["<end_of_turn>", "</s>"],
    }
    t0 = time.monotonic()
    resp = helper.request(req, GEN_TIMEOUT)
    secs = time.monotonic() - t0
    if resp.get("type") == "error":
        raise ProbeError(f"helper error: {resp.get('message')}")
    if resp.get("type") != "response" or resp.get("error"):
        raise ProbeError(f"generation failed: {resp.get('error') or resp}")
    return resp.get("text") or "", secs


def analyse(text):
    hebrew = len(re.findall("[א-ת]", text))
    latin = len(re.findall("[A-Za-z]", text))
    return {
        "words": len(text.split()),
        "hebrew_ratio": hebrew / (hebrew + latin) if hebrew + latin else 0.0,
        "headings": [h for h in HEADINGS if re.search(r"^#{1,6}\s*" + re.escape(h), text, re.M)],
        "facts": [f for f in FACTS if f in text],
        "markers": [m for m in MARKERS if m in text],
        "bullets": len(re.findall(r"^\s*(?:[-*•]|\d+[.)])\s+\S", text, re.M)),
    }


def verdict(a):
    reasons = []
    if a["hebrew_ratio"] < 0.85:
        reasons.append(f"hebrew_ratio {a['hebrew_ratio']:.2f} < 0.85")
    if len(a["headings"]) < 3:
        reasons.append(f"only {len(a['headings'])}/4 headings")
    if a["markers"]:
        reasons.append("reasoning markers present")
    if a["words"] < 60:
        reasons.append(f"only {a['words']} words (< 60)")
    return ("FAIL: " + "; ".join(reasons)) if reasons else "PASS"


def tail(f, n=30):
    f.flush()
    f.seek(0)
    return "\n".join(f.read().decode("utf-8", "replace").splitlines()[-n:])


def write_report(path, args, label, text, a, t_first, t_second, error, stderr_tail):
    out = [f"\n## Hebrew summary: {label}\n"]
    if a:
        gen_secs = t_second if t_second else t_first
        basis = "generate-only" if t_second else "load+generate"
        wps = f"{a['words'] / gen_secs:.2f}" if gen_secs else "n/a"
        fmt = lambda v: "n/a" if v is None else f"{v:.1f}"
        out += ["| Metric | Value |", "|---|---|",
                f"| Model file | `{Path(args.model).name}` |",
                f"| Load+generate seconds | {fmt(t_first)} |",
                f"| Generate-only seconds | {fmt(t_second)} |",
                f"| Words | {a['words']} |",
                f"| Est. words/s ({basis}; estimate, not tokens) | {wps} |",
                f"| Hebrew ratio | {a['hebrew_ratio']:.3f} |",
                f"| Headings found | {len(a['headings'])}/4 |",
                f"| Facts found | {len(a['facts'])}/6 ({', '.join(a['facts']) or 'none'}) |",
                f"| Bullets | {a['bullets']} |",
                f"| Reasoning markers | {'yes' if a['markers'] else 'no'} |", ""]
        out.append(f"**{verdict(a)}**\n")
    if error:
        out.append(f"**ERROR: {error}**\n")
    out += ["```text", text.rstrip(), "```", "",
            "<details><summary>llama-helper stderr (last 30 lines)</summary>", "",
            "```text", stderr_tail, "```", "", "</details>", ""]
    with open(args.report, "a", encoding="utf-8") as f:
        f.write("\n".join(out))


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--helper", required=True)
    ap.add_argument("--model", required=True)
    ap.add_argument("--transcript", required=True)
    ap.add_argument("--report", required=True)
    ap.add_argument("--max-tokens", type=int, default=700)
    ap.add_argument("--label", default=None)
    ap.add_argument("--twice", action=argparse.BooleanOptionalAction, default=True,
                    help="repeat the request to time generation without model loading")
    args = ap.parse_args()
    label = args.label or Path(args.model).stem
    transcript = Path(args.transcript).read_text(encoding="utf-8").strip()

    text, analysis, t_first, t_second, error = "", None, None, None, None
    stderr_file = tempfile.TemporaryFile()
    helper = None
    try:
        helper = Helper(args.helper, stderr_file)
        if helper.request({"type": "ping"}, PING_TIMEOUT).get("type") != "pong":
            raise ProbeError("ping did not return pong")
        text, t_first = generate(helper, args, transcript)
        analysis = analyse(text)
        if args.twice:
            _, t_second = generate(helper, args, transcript)
    except (ProbeError, OSError) as e:
        error = str(e)
    finally:
        if helper:
            helper.close()
        write_report(args.report, args, label, text, analysis, t_first, t_second,
                     error, tail(stderr_file))
        stderr_file.close()

    if error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 2
    result = verdict(analysis)
    print(f"{label}: {result}")
    return 0 if result == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main())
