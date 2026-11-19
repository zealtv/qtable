# The qtable icon

The icon puts the Noto Emoji satellite (U+1F6F0) in front of the Noto Emoji rainbow (U+1F308), as one SVG.

| File | What it is |
|---|---|
| `qtable-icon.svg` | The source: the rainbow, then the satellite at 0.85 scale, centred, in a 144 unit box with an 8 unit margin. |
| `sources/` | The two Noto images exactly as the Noto Emoji project ships them, with its licence file. |
| `build.sh` | Rebuilds everything generated below from `qtable-icon.svg`. macOS only (AppKit renders the SVG, `iconutil` builds the `.icns`). |
| `render.swift` | The rasteriser `build.sh` calls. |
| `qtable.icns` | The macOS app icon: 16, 32, 64, 128, 256, 512 and 1024 px, with the `@2x` pairs. `tools/package-macos.sh` puts it in `qtable.app`. |
| `qtable-512.png` | The Linux desktop icon. |
| `../../crates/qtable-gui/assets/window-icon.rgba` | 128 by 128 raw RGBA for the window icon, embedded in the GUI. |

Rebuild after changing the SVG:

    assets/icon/build.sh

## Licence

Both images are Copyright 2013 Google, Inc., under the Apache License, Version 2.0 (`sources/LICENSE-noto-emoji`;
the full text is in `THIRD_PARTY_NOTICES.md`). qtable is otherwise GPL-3.0-or-later.

## Where each platform gets its icon

- **macOS**: the Dock and Finder read `qtable.icns` from `qtable.app` in the release zip. A bare `qtable-gui`
  binary has no bundle, so it shows the generic icon.
- **Linux**: the window icon comes from the embedded RGBA (shown under X11). For a launcher entry, and for
  the icon under Wayland, install the desktop file and the PNG (the window's application id is `qtable`):

      install -Dm644 assets/linux/qtable.desktop ~/.local/share/applications/qtable.desktop
      install -Dm644 assets/icon/qtable-512.png ~/.local/share/icons/hicolor/512x512/apps/qtable.png

  `Exec=qtable-gui` expects the binary on your `PATH`; edit the line to a full path otherwise.
