#!/usr/bin/env python3
"""The echo program of the input probe: `cat` on a PTY in raw mode.

Bytes on stdin go to the PTY; `cat` writes them back, as a TUI in raw mode
echoes what is typed; the PTY's output goes to stdout. The fake core relays
both ends to the page over its WebSocket.
"""

import os
import pty
import select
import sys
import tty

pid, fd = pty.fork()
if pid == 0:
    tty.setraw(0)
    os.execvp("cat", ["cat"])

try:
    while True:
        r, _, _ = select.select([0, fd], [], [])
        if 0 in r:
            data = os.read(0, 65536)
            if not data:
                break
            os.write(fd, data)
        if fd in r:
            try:
                data = os.read(fd, 65536)
            except OSError:
                break
            if not data:
                break
            os.write(1, data)
finally:
    try:
        os.kill(pid, 15)
    except ProcessLookupError:
        pass
    sys.exit(0)
