# Iskra

Iskra is an experimental operating system based on [Redox OS](https://www.redox-os.org). It is an independent fork and is not affiliated with or endorsed by the Redox OS nonprofit.

The goal is an OS where AI agents run inside capability sandboxes enforced by the microkernel: an agent sees only the files and services it was granted, and asks before reaching for more.

Status: early. Today Iskra is the upstream desktop image with its own identity (`config/iskra.toml`): synthwave theme, Firefox and WebKitGTK, and an `ai` command that talks to Claude, OpenAI, Gemini or local models. See `PATCHES.md` for every change to upstream files and `EXPERIMENTS.md` for parked work (XFCE and xfwl4 on Redox).

## Build and test

Needs a Linux host (x86_64 recommended; arm64 works for building) with `sudo`.

```bash
./iskra.sh setup   # install podman, QEMU and friends (Debian/Ubuntu)
./iskra.sh build   # build build/x86_64/iskra/harddrive.img
./iskra.sh test    # headless boot; passes when the login prompt appears on serial
./iskra.sh run     # boot with the desktop on VNC at 127.0.0.1:5900
./iskra.sh check   # report CPU, KVM and nested-virtualization support
```

On a Proxmox server, see `PROXMOX.md`.

`run` and `test` use KVM when the host is x86_64 with `/dev/kvm`, and software emulation otherwise.

To see the desktop from another machine: `ssh -L 5900:127.0.0.1:5900 <server>`, then open `vnc://localhost:5900`.

## Branches

- `iskra`: Iskra work.
- `master`: tracks upstream Redox unchanged.

## Licence

MIT, as upstream. See `LICENSE`. "Redox OS" is a trademark of the Redox OS nonprofit; see `TRADEMARK.md`.
