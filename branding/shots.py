#!/usr/bin/env python3
"""Boot the Iskra image headless with a virtual VGA display, log in through the GUI
by sending keystrokes, and save screenshots of the login screen and the desktop.

Usage: [ISKRA_CONFIG=iskra] [SHOTS_NO_LOGIN=1 SHOTS_DESKTOP_WAIT=120] branding/shots.py OUTDIR [steps...]
Each extra step is a number (seconds to wait), "key:<qemu sendkey names>"
(e.g. key:ctrl-t, key:ret), "type:<text>", "mouse:<dx>,<dy>" (relative move),
"click", or "shot:<name>" to save a screenshot.
Runs from the repo root; uses upstream `make qemu`.
"""
import os, select, subprocess, sys, time

out = os.path.abspath(sys.argv[1]); os.makedirs(out, exist_ok=True)
steps = sys.argv[2:]
os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
p = subprocess.Popen(
    ["make", "qemu", "ARCH=x86_64", "CONFIG_NAME=" + os.environ.get("ISKRA_CONFIG", "iskra"),
     "QEMU=qemu-system-x86_64 -snapshot -display none -m 4096"],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
    start_new_session=True)
log = open(os.path.join(out, "serial.log"), "wb")
seen = b""

def pump(seconds, marker=None):
    global seen
    end = time.time() + seconds
    while time.time() < end:
        r, _, _ = select.select([p.stdout], [], [], 0.5)
        if r:
            chunk = os.read(p.stdout.fileno(), 65536)
            if not chunk:
                return False
            log.write(chunk); log.flush(); seen += chunk
            if marker and marker in seen:
                return True
    return marker is None

def monitor(cmd):
    # The serial console and QEMU monitor share stdio; Ctrl-A c toggles between them.
    p.stdin.write(b"\x01c"); p.stdin.flush(); time.sleep(0.3)
    p.stdin.write(cmd.encode() + b"\n"); p.stdin.flush(); time.sleep(0.5)
    p.stdin.write(b"\x01c"); p.stdin.flush(); time.sleep(0.3)

def shot(name):
    path = os.path.join(out, name + ".ppm")
    monitor(f"screendump {path}"); pump(2)
    subprocess.run(["convert", path, path[:-4] + ".png"], check=False)
    print("saved", path[:-4] + ".png", flush=True)

def keys(names):
    for k in names.split():
        monitor(f"sendkey {k}"); time.sleep(0.15)

KEYMAP = {" ": "spc", "-": "minus", ".": "dot", "/": "slash", "_": "shift-minus", ":": "shift-semicolon"}
def type_text(text):
    keys(" ".join(KEYMAP.get(c, ("shift-" + c.lower()) if c.isupper() else c) for c in text))

try:
    # With a display attached the bootloader waits for a resolution choice.
    assert pump(300, b"1920x1080"), "no resolution menu"
    pump(2); keys("ret")
    assert pump(900, b"login:"), "no login prompt on serial"
    if os.environ.get("SHOTS_NO_LOGIN"):   # session starts without orblogin (e.g. iskra-xfce)
        pump(float(os.environ.get("SHOTS_DESKTOP_WAIT", "120")))
    else:
        pump(25)                      # let orblogin draw
        shot("1-login")
        keys("ret")                   # username is pre-filled as "user"; focus is on the empty password
        pump(40)                      # desktop and launcher start
    shot("2-desktop")
    for s in steps:
        if s.startswith("key:"): keys(s[4:])
        elif s.startswith("type:"): type_text(s[5:])
        elif s.startswith("shot:"): shot(s[5:])
        elif s.startswith("mouse:"):   # relative move in pixels, e.g. mouse:-3000,3000 to reach bottom-left
            dx, dy = s[6:].split(","); monitor(f"mouse_move {dx} {dy}")
        elif s == "click": monitor("mouse_button 1"); time.sleep(0.2); monitor("mouse_button 0")
        else: pump(float(s))
finally:
    os.killpg(p.pid, 9)
