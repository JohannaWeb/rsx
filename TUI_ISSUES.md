# Build Fixes

### Sound / ALSA Issue
If you get an error about `alsa-sys` or `alsa.pc` missing, run:

```bash
sudo apt update && sudo apt install -y libasound2-dev
```

### GUI / Windowing Issue (Minifb)
If you run into issues with the window not opening or X11 errors:

```bash
sudo apt install -y libx11-dev libxkbcommon-dev
```
