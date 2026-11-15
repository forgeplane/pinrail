#!/usr/bin/env python3
"""Scrubs an asciicast v2 recording of names, paths, emails and secrets.

    python3 corpus/tools/scrub.py IN.cast OUT.cast [--replace OLD=NEW ...]

Every replacement keeps the length of the text it replaces (shorter
replacements are padded with spaces), so the positions of a TUI's cells do
not move. Built-in replacements: the user name, the home folder, the host
name, session ids (UUIDs), and any `--replace` pairs. After replacing, the
output is scanned for what still looks like a secret (API keys, tokens,
private keys, emails, home folders), and each finding is printed. The exit status is 1 when
anything was found, so a person reviews it before the file is kept.
"""

import getpass
import json
import os
import re
import socket
import sys

SUSPECT = [
    ("api key", re.compile(r"\b(sk|rk|pk)-[A-Za-z0-9_-]{16,}")),
    ("anthropic key", re.compile(r"sk-ant-[A-Za-z0-9_-]{10,}")),
    ("github token", re.compile(r"\b(ghp|gho|ghu|ghs|ghr)_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}")),
    ("slack token", re.compile(r"\bxox[abprs]-[A-Za-z0-9-]{10,}")),
    ("aws key", re.compile(r"\bAKIA[0-9A-Z]{16}\b")),
    ("jwt", re.compile(r"\beyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}")),
    ("bearer", re.compile(r"[Bb]earer [A-Za-z0-9._-]{16,}")),
    ("private key", re.compile(r"-----BEGIN [A-Z ]*PRIVATE KEY-----")),
    ("email", re.compile(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b")),
    ("home folder", re.compile(r"/Users/(?!person\b)[A-Za-z0-9._-]+|/home/(?!user\b)[A-Za-z0-9._-]+")),
    ("long hex", re.compile(r"\b[0-9a-f]{32,}\b")),
]
UUID = re.compile(r"\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b")
ZERO_UUID = "00000000-0000-0000-0000-000000000000"
ALLOWED_EMAILS = {"user@example.org", "noreply@anthropic.com"}


def same_length(old, new):
    if len(new) > len(old):
        raise SystemExit(f"replacement for {old!r} is longer than the original")
    return new + " " * (len(old) - len(new))


def main():
    args = sys.argv[1:]
    if len(args) < 2:
        raise SystemExit(__doc__)
    src, dst, rest = args[0], args[1], args[2:]
    user = getpass.getuser()
    host = socket.gethostname().split(".")[0]
    pairs = []
    i = 0
    while i < len(rest):
        if rest[i] == "--replace":
            old, new = rest[i + 1].split("=", 1)
            pairs.append((old, new))
            i += 2
        else:
            raise SystemExit(f"unknown argument {rest[i]}")
    pairs += [
        (os.path.expanduser("~"), "/Users/person"),
        (user, "person"),
        (host, "host"),
    ]
    # Longest first, so a path is replaced before the name inside it.
    pairs = sorted({(o, same_length(o, n)) for o, n in pairs if o}, key=lambda p: -len(p[0]))

    out_lines = []
    findings = []
    with open(src, encoding="utf-8") as f:
        header = json.loads(f.readline())
        for k in ("title", "command"):
            if k in header:
                for o, n in pairs:
                    header[k] = header[k].replace(o, n.rstrip())
        header.get("env", {}).pop("HOME", None)
        out_lines.append(json.dumps(header, ensure_ascii=False))
        for n, line in enumerate(f, 2):
            t, kind, data = json.loads(line)
            for o, r in pairs:
                data = data.replace(o, r)
            data = UUID.sub(ZERO_UUID, data)
            for name, rx in SUSPECT:
                for m in rx.finditer(data):
                    if name == "email" and m.group(0) in ALLOWED_EMAILS:
                        continue
                    findings.append((n, name, m.group(0)[:60]))
            out_lines.append(json.dumps([t, kind, data], ensure_ascii=False))
    with open(dst, "w", encoding="utf-8") as f:
        f.write("\n".join(out_lines) + "\n")
    for n, name, text in findings:
        print(f"{dst}:{n}: {name}: {text}")
    print(f"wrote {dst}: {len(out_lines) - 1} events, {len(findings)} findings", file=sys.stderr)
    sys.exit(1 if findings else 0)


if __name__ == "__main__":
    main()
