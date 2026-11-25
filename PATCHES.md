# Changes to upstream files

Every upstream file Iskra modifies is listed here, with the reason. New Iskra-only files are not listed.

| File | Change | Why |
|---|---|---|
| `README.md` | Notice at the top pointing to `ISKRA.md` | So the repository front page does not present Iskra as Redox OS (see `TRADEMARK.md`). |
| `recipes/gui/orbdata/recipe.toml` | After copying upstream's UI data, copy `branding/ui/*` over `/usr/share/ui/` if that folder exists | Iskra wallpaper and login screen. Config `[[files]]` entries can only hold text, and a second package shipping the same paths would conflict. No effect on builds without `branding/ui`. |

Iskra-only files: `ISKRA.md`, `PATCHES.md`, `iskra.sh`, `config/iskra.toml`, `branding/` (`synthwave.sh` renders the artwork with ImageMagick; `ui/` holds the output).
