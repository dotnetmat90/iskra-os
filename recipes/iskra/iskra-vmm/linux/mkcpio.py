#!/usr/bin/env python3
"""Write a newc initramfs to stdout.

    mkcpio.py INIT [BUSYBOX]

INIT becomes /init (a static binary or a script for BusyBox). With BUSYBOX, /bin/busybox is
added. Directories and /dev/console, /dev/kmsg, /dev/ttyS0 nodes are written into the archive
directly, so no root is needed."""
import sys

def entry(name, mode, data=b"", rdev=(0, 0), ino=[0]):
    ino[0] += 1
    name_b = name.encode() + b"\0"
    fields = [ino[0], mode, 0, 0, 1, 0, len(data), 0, 0, rdev[0], rdev[1], len(name_b), 0]
    out = b"070701" + b"".join(b"%08X" % f for f in fields) + name_b
    out += b"\0" * (-len(out) % 4) + data
    return out + b"\0" * (-len(out) % 4)

parts = [entry(d, 0o040755) for d in ("bin", "dev", "proc", "sys", "tmp", "root")]
parts += [
    entry("dev/console", 0o020600, rdev=(5, 1)),
    entry("dev/kmsg", 0o020644, rdev=(1, 11)),
    entry("dev/ttyS0", 0o020660, rdev=(4, 64)),
    entry("init", 0o100755, open(sys.argv[1], "rb").read()),
]
if len(sys.argv) > 2:
    parts.append(entry("bin/busybox", 0o100755, open(sys.argv[2], "rb").read()))
archive = b"".join(parts) + entry("TRAILER!!!", 0)
sys.stdout.buffer.write(archive + b"\0" * (-len(archive) % 512))
