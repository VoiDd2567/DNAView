# DNAView

DNAView is a cross-platform Rust desktop application for visualizing a simplified 3D DNA double helix.

## Features

- `winit` windowing with a custom `wgpu` renderer
- `egui` text input and status UI
- DNA validation for `A`, `T`, `C`, `G`, and unknown `N`
- Whitespace is ignored while spaces and newlines are typed or pasted
- DNA input automatically groups bases every 4 letters
- Automatic complementary strand generation
- Full-sequence rendering for every valid base pair
- Configurable helix geometry and base colors through TOML
- Light-blue gradient scene background
- High-resolution DNA spheres and rods
- Specular light source highlights on the DNA model
- Mouse wheel zoom with optional background auto-rotation
- Orthographic camera keeps DNA size stable while changing angles
- Optional background auto-rotation
- Fullscreen DNA-only viewing mode
- Live settings window for geometry and color config
- Right-side straight DNA overview shows a configurable number of pairs with a scrollbar for longer sequences
- Local sequence save/load from the `sequences/` folder
- Rectangle selection overlay with selected base-pair highlighting

## Configuration

Edit `config.toml`:

```toml
visible_pairs = 40 # kept for older configs; the app renders the full sequence
map_visible_pairs = 50
radius = 0.75
vertical_spacing = 0.16
angle_step = 0.36
sphere_radius = 0.24
stick_radius = 0.018

[colors]
A = "#ff4444"
T = "#44ff44"
C = "#4444ff"
G = "#ffff44"
N = "#888888"
```

## Run

```bash
cargo run
```

## Build

```bash
cargo build --release
```

## Controls

- Mouse wheel: zoom in or out
- Left mouse drag: move camera on screen X/Y
- Shift + left mouse drag: larger horizontal movement leans the view, larger vertical movement tilts the DNA model
- Auto zoom button: recenter and fit the DNA in view
- Reset angles button: restore straight DNA/camera angles
- Fullscreen button: show only the DNA view
- Escape: leave fullscreen
- Settings button: edit live geometry and colors
- Auto rotate checkbox: turn background rotation on or off
- Ctrl + left mouse drag: draw a selection rectangle

## Created

Project is fully written by AI, but code is reviewed by a human.
