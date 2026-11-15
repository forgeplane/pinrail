#!/usr/bin/env python3
"""Records a terminal program in a PTY as an asciicast v2 file.

Interactive (you drive the program; resizing your terminal is recorded):

    python3 corpus/tools/record.py -o out.cast -- claude

Unattended, following a plan of steps (see plans/*.json):

    python3 corpus/tools/record.py -o out.cast --plan plan.json --cwd DIR -- claude "prompt"

The child gets a small, fixed environment (PATH, HOME, USER, SHELL, LANG,
TERM, TMPDIR), never the caller's tokens or the variables of an enclosing
agent session. Output events are "o", resizes "r", plan markers "m". Typed
input is not recorded. Run scrub.py on the result before keeping it.

Plan steps, run in order:
  {"sleep": 2}
  {"send": "text\\r"}                    keys, JSON escapes allowed
  {"expect": "regex", "timeout": 60}     waits for output matching regex
  {"resize": [100, 30]}
  {"idle": 5, "timeout": 300, "answer": [["regex", "keys"], ...]}
      waits until there has been no output for `idle` seconds; meanwhile,
      each time new output matches an answer's regex, sends its keys
  {"marker": "text"}
"""

import argparse
import fcntl
import json
import os
import pty
import re
import select
import signal
import struct
import sys
import termios
import time
import tty

ANSI = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b[@-Z\\-_]")


MOVE = re.compile(rb"\x1b\[\d*[CG]")
SPACES = re.compile(rb" +")


def plain_text(data):
    """The output without escapes, for matching: cursor moves along the line
    become one space, and runs of spaces are collapsed."""
    return SPACES.sub(b" ", ANSI.sub(b"", MOVE.sub(b" ", data)))


def set_size(fd, cols, rows):
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))


class Recorder:
    def __init__(self, path, cols, rows, title):
        self.f = open(path, "w", encoding="utf-8")
        self.t0 = time.monotonic()
        header = {
            "version": 2,
            "width": cols,
            "height": rows,
            "timestamp": int(time.time()),
            "env": {"TERM": "xterm-256color", "SHELL": "/bin/zsh"},
        }
        if title:
            header["title"] = title
        self.f.write(json.dumps(header) + "\n")
        self.decoder_buf = b""

    def event(self, kind, data):
        t = round(time.monotonic() - self.t0, 6)
        self.f.write(json.dumps([t, kind, data], ensure_ascii=False) + "\n")

    def output(self, data):
        # Keep incomplete UTF-8 sequences for the next event.
        buf = self.decoder_buf + data
        cut = len(buf)
        for back in range(1, min(4, len(buf)) + 1):
            b = buf[-back]
            if b & 0xC0 == 0x80:
                continue
            need = 1 if b < 0x80 else 2 if b >> 5 == 0b110 else 3 if b >> 4 == 0b1110 else 4 if b >> 3 == 0b11110 else 1
            if need > back:
                cut = len(buf) - back
            break
        self.decoder_buf = buf[cut:]
        if cut:
            self.event("o", buf[:cut].decode("utf-8", errors="replace"))

    def close(self):
        if self.decoder_buf:
            self.event("o", self.decoder_buf.decode("utf-8", errors="replace"))
        self.f.close()


def child_env():
    keep = ("PATH", "HOME", "USER", "LOGNAME", "SHELL", "TMPDIR")
    env = {k: os.environ[k] for k in keep if k in os.environ}
    env.update(TERM="xterm-256color", COLORTERM="truecolor", LANG="en_US.UTF-8")
    return env


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("-o", "--out", required=True)
    p.add_argument("--plan")
    p.add_argument("--cwd")
    p.add_argument("--cols", type=int, default=120)
    p.add_argument("--rows", type=int, default=40)
    p.add_argument("--title")
    p.add_argument("--max-seconds", type=float, default=900)
    p.add_argument("cmd", nargs=argparse.REMAINDER)
    a = p.parse_args()
    cmd = a.cmd[1:] if a.cmd and a.cmd[0] == "--" else a.cmd
    if not cmd:
        p.error("no command given")

    interactive = a.plan is None
    cols, rows = a.cols, a.rows
    if interactive and os.isatty(0):
        rows, cols = struct.unpack("HHHH", fcntl.ioctl(0, termios.TIOCGWINSZ, b"\0" * 8))[:2]

    pid, fd = pty.fork()
    if pid == 0:
        if a.cwd:
            os.chdir(a.cwd)
        os.execvpe(cmd[0], cmd, child_env())
    set_size(fd, cols, rows)
    os.kill(pid, signal.SIGWINCH)
    rec = Recorder(a.out, cols, rows, a.title)
    recent = b""  # output since the last plan check, without escapes
    last_output = time.monotonic()
    done = False

    def pump(timeout):
        nonlocal recent, last_output, done
        fds = [fd] + ([0] if interactive else [])
        r, _, _ = select.select(fds, [], [], timeout)
        if fd in r:
            try:
                data = os.read(fd, 65536)
            except OSError:
                data = b""
            if not data:
                done = True
                return
            rec.output(data)
            if interactive:
                os.write(1, data)
            recent = (recent + plain_text(data))[-65536:]
            last_output = time.monotonic()
        if interactive and 0 in r:
            data = os.read(0, 4096)
            if data:
                os.write(fd, data)

    def send(keys):
        os.write(fd, keys.encode("utf-8"))

    def resize(c, r):
        set_size(fd, c, r)
        os.kill(pid, signal.SIGWINCH)
        rec.event("r", f"{c}x{r}")

    start = time.monotonic()
    if interactive:
        old = termios.tcgetattr(0) if os.isatty(0) else None

        def on_winch(*_):
            r_, c_ = struct.unpack("HHHH", fcntl.ioctl(0, termios.TIOCGWINSZ, b"\0" * 8))[:2]
            resize(c_, r_)

        signal.signal(signal.SIGWINCH, on_winch)
        if old:
            tty.setraw(0)
        try:
            while not done:
                pump(0.5)
        finally:
            if old:
                termios.tcsetattr(0, termios.TCSAFLUSH, old)
    else:
        with open(a.plan) as f:
            plan = json.load(f)
        for step in plan:
            if done or time.monotonic() - start > a.max_seconds:
                break
            print("step", json.dumps(step)[:100], file=sys.stderr)
            if "sleep" in step:
                end = time.monotonic() + step["sleep"]
                while not done and time.monotonic() < end:
                    pump(max(0.0, min(0.2, end - time.monotonic())))
            elif "send" in step:
                recent = b""
                send(step["send"])
            elif "resize" in step:
                resize(*step["resize"])
            elif "marker" in step:
                rec.event("m", step["marker"])
            elif "expect" in step:
                rx = re.compile(step["expect"].encode())
                end = time.monotonic() + step.get("timeout", 60)
                while not done and not rx.search(recent) and time.monotonic() < end:
                    pump(0.2)
                if not rx.search(recent):
                    print("  timed out waiting for", step["expect"], file=sys.stderr)
                    if step.get("required"):
                        break
                recent = b""
            elif "idle" in step:
                answers = [(re.compile(rx.encode()), keys) for rx, keys in step.get("answer", [])]
                end = time.monotonic() + step.get("timeout", 300)
                while not done and time.monotonic() < end:
                    pump(0.2)
                    for rx, keys in answers:
                        if rx.search(recent):
                            print("  answering", rx.pattern, file=sys.stderr)
                            recent = b""
                            time.sleep(0.6)
                            send(keys)
                            last_output = time.monotonic()
                            break
                    if time.monotonic() - last_output > step["idle"]:
                        break
        # Give the program a moment to exit, then end it.
        end = time.monotonic() + 5
        while not done and time.monotonic() < end:
            pump(0.2)
        if not done:
            os.kill(pid, signal.SIGTERM)
            end = time.monotonic() + 3
            while not done and time.monotonic() < end:
                pump(0.2)
    rec.close()
    try:
        os.waitpid(pid, 0)
    except ChildProcessError:
        pass
    print("wrote", a.out, file=sys.stderr)


if __name__ == "__main__":
    main()
