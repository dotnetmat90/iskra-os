#!/usr/bin/env python3
"""Write a newc initramfs to stdout: /init (from argv[1]), /dev/console and /dev/kmsg nodes.
Device nodes are written into the archive directly, so no root is needed."""
import sys

def entry(name, mode, data=b"", rdev=(0, 0), ino=[0]):
    ino[0] += 1
    name_b = name.encode() + b"\0"
    fields = [ino[0], mode, 0, 0, 1, 0, len(data), 0, 0, rdev[0], rdev[1], len(name_b), 0]
    out = b"070701" + b"".join(b"%08X" % f for f in fields) + name_b
    out += b"\0" * (-len(out) % 4) + data
    return out + b"\0" * (-len(out) % 4)

init = open(sys.argv[1], "rb").read()
archive = (
    entry("dev", 0o040755)
    + entry("dev/console", 0o020600, rdev=(5, 1))
    + entry("dev/kmsg", 0o020644, rdev=(1, 11))
    + entry("init", 0o100755, init)
    + entry("TRAILER!!!", 0)
)
sys.stdout.buffer.write(archive + b"\0" * (-len(archive) % 512))
