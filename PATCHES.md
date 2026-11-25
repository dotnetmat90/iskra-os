# Changes to upstream files

Every upstream file Iskra modifies is listed here, with the reason. New Iskra-only files are not listed.

| File | Change | Why |
|---|---|---|
| `README.md` | Notice at the top pointing to `ISKRA.md` | So the repository front page does not present Iskra as Redox OS (see `TRADEMARK.md`). |
| `recipes/gui/orbdata/recipe.toml` | After copying upstream's data, copy `branding/share/` over `/usr/share/` if that folder exists | Iskra wallpaper, login screen, start-button icon and Orbital colours. Config `[[files]]` entries can only hold text, and a second package shipping the same paths would conflict. No effect on builds without `branding/share`. |

Iskra-only files: `ISKRA.md`, `PATCHES.md`, `iskra.sh`, `config/iskra.toml`, and `branding/`:

- `branding/share/` mirrors `/usr/share/`; every file in it replaces upstream's.
- `branding/synthwave.sh` renders the wallpaper and login art with ImageMagick 7.
- `branding/shots.py` boots the image headless and saves screenshots (login, desktop, and any keyboard/mouse steps you pass).
