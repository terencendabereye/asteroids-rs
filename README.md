# Asteroids (Rust port)

Same game as the [C++/raylib version](https://github.com/terencendabereye/asteroids) - Classic and UFO modes, pause / game-over / settings screens, remappable keyboard + gamepad controls, three aim modes (rotate keys / right stick / mouse), DualSense rumble and adaptive triggers - rewritten in Rust on [macroquad](https://github.com/not-fl3/macroquad) (graphics, keyboard, mouse), [gilrs](https://crates.io/crates/gilrs) (gamepads) and [hidapi](https://crates.io/crates/hidapi) (DualSense feedback). All dependencies are plain Cargo crates: **no CMake, vcpkg, LLVM, C compiler or system libraries on Windows.**

## Build and run

### Windows
1. Install Rust from <https://rustup.rs>. When it asks about the MSVC linker, accept the *Visual Studio C++ Build Tools* install (rustc itself needs a linker on Windows; nothing else is required).
2. In the project folder:
   ```
   cargo run --release
   ```
That's it - cargo downloads and builds everything.

### Linux (Fedora)
```
sudo dnf install systemd-devel     # libudev headers, needed by gilrs
cargo run --release
```
For DualSense feedback on Linux the pad's hidraw node must be accessible to your user (a udev rule, or run with access to `/dev/hidraw*`).

## Controls (remappable in Settings)
Rotate: Left/Right (D-pad or left stick). Accelerate / Brake: Up/Down. Fire: Space (A, or R2 on a gamepad). Pause: Esc / Start. Menus: arrows/W/S + Enter, or D-pad + A (B/Esc = back). Bindings and options are saved to `controls.cfg` in the working directory.

**Settings -> Aim with** cycles *Rotate keys* / *Right stick* / *Mouse* (ship faces the cursor, left click fires). In the stick and mouse modes the rotate keys are ignored.

## Class hierarchy
Rust has no class inheritance, so the tree is a chain of traits - each a supertrait of the one above - with shared state in a `RigidBody` struct (`src/objects.rs`):
```
GameObject            position, update(dt), alive
├─ Drawable           colour, draw()
│  ├─ Menu            UI list (src/ui.rs)
│  └─ Body            owns a RigidBody: velocity, rotation, spin, radius, screen wrap
│     ├─ Asteroid
│     ├─ Laser
│     ├─ Ufo
│     └─ Controllable handle_input()
│        └─ Player    score, lives, invulnerability
│           └─ Spaceship
```

## Notes
- DualSense rumble and adaptive triggers are sent over HID using the report layout from the Linux kernel and SDL. **This has not been verified on real hardware.** Steam Input can take over the controller and override it.
- On Windows gilrs reads gamepads through Windows.Gaming.Input; whether a DualSense shows up as a usable gamepad there is untested. (Keyboard and mouse always work.)
- `cargo test` runs a few unit tests (CRC32 for the Bluetooth report, object behaviour).

## License
MIT - see [LICENSE](LICENSE).
