//! Controller feedback: rumble and DualSense adaptive triggers, sent straight to the pad over HID.
//! (Neither macroquad nor gilrs can vibrate a DualSense.) Report layout is taken from the Linux
//! kernel (drivers/hid/hid-playstation.c) and SDL (SDL_hidapi_ps5.c):
//!   common block: 0 flags0, 1 flags1, 2 motor right (light), 3 motor left (heavy),
//!                 10..20 right-trigger effect (11 bytes)
//!   USB: report 0x02, 63 bytes, common at byte 1.
//!   Bluetooth: report 0x31, 78 bytes, [1]=seq<<4, [2]=0x10, common at byte 3, CRC32 (seed 0xA2)
//!              of the first 74 bytes in the last 4.
//! Written from those sources; not verified on real hardware.

use gilrs::Button;

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum TriggerMode { Off, Resist }

/// Controller state read straight from the DualSense's HID input reports. Needed because
/// Windows' gamepad APIs (which gilrs uses) only expose Xbox-style pads, so without a wrapper
/// such as Steam Input a DualSense is invisible to gilrs. Sticks are -1..1, up is positive.
#[derive(Clone, Default, Debug)]
pub struct PadSnapshot {
    pub down: Vec<Button>,
    pub lx: f32,
    pub ly: f32,
    pub rx: f32,
    pub ry: f32,
}

/// Parse a DualSense input report. Layouts from the Linux kernel / SDL:
///   USB 0x01 (64 B) and Bluetooth full 0x31 (78 B): sticks/triggers/buttons start at byte 1 / 2.
///   Bluetooth reduced 0x01 (10 B): different, shorter layout.
#[allow(dead_code)]
pub fn parse_report(r: &[u8]) -> Option<PadSnapshot> {
    let (sticks, b0, b1, b2) = match (r.first()?, r.len()) {
        (0x31, n) if n >= 12 => (2, r[9], r[10], r[11]),
        (0x01, n) if n >= 64 => (1, r[8], r[9], r[10]),
        (0x01, n) if n >= 10 => (1, r[5], r[6], r[7]), // reduced BT report
        _ => return None,
    };
    let ax = |i: usize| ((r[sticks + i] as f32 - 128.0) / 127.0).clamp(-1.0, 1.0);
    let mut s = PadSnapshot { lx: ax(0), ly: -ax(1), rx: ax(2), ry: -ax(3), down: Vec::new() };
    let mut set = |on: bool, b: Button| if on { s.down.push(b); };
    let hat = b0 & 0x0F;
    set(matches!(hat, 7 | 0 | 1), Button::DPadUp);
    set(matches!(hat, 1 | 2 | 3), Button::DPadRight);
    set(matches!(hat, 3 | 4 | 5), Button::DPadDown);
    set(matches!(hat, 5 | 6 | 7), Button::DPadLeft);
    set(b0 & 0x10 != 0, Button::West);   // square
    set(b0 & 0x20 != 0, Button::South);  // cross
    set(b0 & 0x40 != 0, Button::East);   // circle
    set(b0 & 0x80 != 0, Button::North);  // triangle
    set(b1 & 0x01 != 0, Button::LeftTrigger);
    set(b1 & 0x02 != 0, Button::RightTrigger);
    set(b1 & 0x04 != 0, Button::LeftTrigger2);
    set(b1 & 0x08 != 0, Button::RightTrigger2);
    set(b1 & 0x10 != 0, Button::Select);
    set(b1 & 0x20 != 0, Button::Start);
    set(b1 & 0x40 != 0, Button::LeftThumb);
    set(b1 & 0x80 != 0, Button::RightThumb);
    set(b2 & 0x01 != 0, Button::Mode);
    Some(s)
}

pub struct Haptics {
    vibration: bool,
    triggers: bool,
    mode: TriggerMode,
    heavy_lvl: f32,
    heavy_time: f32,
    light_lvl: f32,
    light_time: f32,
    backend: backend::Backend,
}

impl Haptics {
    pub fn new() -> Self {
        Self {
            vibration: true, triggers: true, mode: TriggerMode::Off,
            heavy_lvl: 0.0, heavy_time: 0.0, light_lvl: 0.0, light_time: 0.0,
            backend: backend::Backend::new(),
        }
    }

    pub fn set_vibration_enabled(&mut self, on: bool) { self.vibration = on; }
    pub fn set_triggers_enabled(&mut self, on: bool) { self.triggers = on; }
    pub fn set_trigger(&mut self, m: TriggerMode) { self.mode = m; }

    /// Pulse the heavy (low-frequency) and light (high-frequency) motors, strengths 0..1.
    /// A weaker pulse never cuts short a stronger one that is still running.
    pub fn rumble(&mut self, heavy: f32, light: f32, seconds: f32) {
        if heavy > 0.0 && (heavy >= self.heavy_lvl || self.heavy_time <= 0.0) { self.heavy_lvl = heavy; self.heavy_time = seconds; }
        if light > 0.0 && (light >= self.light_lvl || self.light_time <= 0.0) { self.light_lvl = light; self.light_time = seconds; }
    }

    pub fn stop_rumble(&mut self) {
        self.heavy_lvl = 0.0; self.light_lvl = 0.0; self.heavy_time = 0.0; self.light_time = 0.0;
    }

    pub fn update(&mut self, dt: f32) {
        self.heavy_time -= dt;
        if self.heavy_time <= 0.0 { self.heavy_lvl = 0.0; }
        self.light_time -= dt;
        if self.light_time <= 0.0 { self.light_lvl = 0.0; }

        let motor = |lvl: f32, on: bool| if on { (lvl * 255.0).clamp(0.0, 255.0) as u8 } else { 0 };
        let h = motor(self.heavy_lvl, self.vibration);
        let l = motor(self.light_lvl, self.vibration);
        let m = if self.triggers { self.mode } else { TriggerMode::Off };
        self.backend.tick(dt, h, l, m);
    }

    /// Everything off and the device released (also done on drop).
    pub fn shutdown(&mut self) {
        self.stop_rumble();
        self.mode = TriggerMode::Off;
        self.backend.shutdown();
    }

    pub fn connected(&self) -> bool { self.backend.connected() }

    /// Latest controller state from the HID link, or `None` if no DualSense is open.
    pub fn read_pad(&mut self) -> Option<PadSnapshot> { self.backend.read_pad() }

    /// Human-readable reason/state of the DualSense HID link (for the settings screen).
    pub fn status(&self) -> String { self.backend.status() }
}

/// Lists HID devices relevant to controllers (used by `--diagnose`).
pub fn diagnose() -> Vec<String> { backend::diagnose() }

impl Drop for Haptics {
    fn drop(&mut self) { self.shutdown(); }
}

// ───────────────────────────── HID backend ────────────────────────────
#[cfg(any(target_os = "windows", target_os = "linux"))]
mod backend {
    use super::{parse_report, PadSnapshot, TriggerMode};
    use hidapi::{BusType, HidApi, HidDevice};

    pub struct Backend {
        pad: Option<PadSnapshot>,
        api: Option<HidApi>,
        dev: Option<HidDevice>,
        bluetooth: bool,
        seq: u8,
        retry: f32,
        since_send: f32,
        last: (u8, u8, TriggerMode),
        force: bool,
        status: String,
    }

    pub(super) fn crc32(mut crc: u32, data: &[u8]) -> u32 {
        crc = !crc;
        for &b in data {
            crc ^= b as u32;
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xEDB8_8320 & 0u32.wrapping_sub(crc & 1));
            }
        }
        !crc
    }

    pub fn diagnose() -> Vec<String> {
        let mut out = Vec::new();
        match HidApi::new() {
            Err(e) => out.push(format!("hidapi init failed: {e}")),
            Ok(api) => {
                let mut n = 0;
                for d in api.device_list() {
                    if d.vendor_id() == 0x054C || d.usage_page() == 0x01 && matches!(d.usage(), 0x04 | 0x05) {
                        n += 1;
                        out.push(format!(
                            "  {:04x}:{:04x} usage {:04x}/{:04x} bus {:?} product {:?} path {:?}",
                            d.vendor_id(), d.product_id(), d.usage_page(), d.usage(), d.bus_type(),
                            d.product_string().unwrap_or("?"), d.path()
                        ));
                    }
                }
                if n == 0 { out.push("  (no gamepad-like or Sony HID devices visible)".into()); }
            }
        }
        out
    }

    impl Backend {
        pub fn new() -> Self {
            Self { pad: None, api: None, dev: None, bluetooth: false, seq: 0, retry: 0.0, since_send: 0.0, last: (0, 0, TriggerMode::Off), force: false, status: "searching...".into() }
        }

        pub fn connected(&self) -> bool { self.dev.is_some() }

        /// Drain pending input reports (non-blocking) and return the most recent state.
        pub fn read_pad(&mut self) -> Option<PadSnapshot> {
            let Some(dev) = self.dev.as_ref() else { self.pad = None; return None };
            let mut buf = [0u8; 128];
            loop {
                match dev.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => { if let Some(p) = parse_report(&buf[..n]) { self.pad = Some(p); } }
                    Err(_) => { self.dev = None; self.pad = None; self.last = (0, 0, TriggerMode::Off); return None; }
                }
            }
            if self.pad.is_none() { self.pad = Some(PadSnapshot::default()); } // open, nothing pressed yet
            self.pad.clone()
        }

        pub fn status(&self) -> String { self.status.clone() }

        fn open(&mut self) {
            match self.api.as_mut() {
                Some(api) => { let _ = api.refresh_devices(); }
                None => match HidApi::new() {
                    Ok(api) => self.api = Some(api),
                    Err(e) => { self.status = format!("HID unavailable: {e}"); return; }
                },
            }
            let Some(api) = self.api.as_ref() else { return };
            let mut sony = 0;
            let mut matched = 0;
            let mut open_err = String::new();
            for info in api.device_list() {
                if info.vendor_id() != 0x054C { continue; }
                sony += 1;
                if !matches!(info.product_id(), 0x0CE6 | 0x0DF2) { continue; } // DualSense / Edge
                let any_usage = info.usage_page() == 0 && info.usage() == 0;
                if !any_usage && !(info.usage_page() == 0x01 && info.usage() == 0x05) { continue; } // gamepad collection
                matched += 1;
                match api.open_path(info.path()) {
                    Ok(dev) => {
                        let _ = dev.set_blocking_mode(false);
                        self.bluetooth = matches!(info.bus_type(), BusType::Bluetooth);
                        self.dev = Some(dev);
                        break;
                    }
                    Err(e) => open_err = e.to_string(),
                }
            }
            self.status = if self.dev.is_some() {
                format!("connected ({})", if self.bluetooth { "Bluetooth" } else { "USB" })
            } else if matched > 0 {
                format!("found, but open failed: {open_err} (another app, e.g. Steam, may hold it)")
            } else if sony > 0 {
                format!("{sony} Sony HID interface(s) seen, none is a DualSense gamepad")
            } else {
                "no Sony HID device visible to this OS".to_string()
            };
            if let Some(dev) = self.dev.as_ref() {
                if self.bluetooth {
                    // Over Bluetooth the pad starts in a reduced mode; reading the calibration
                    // feature report switches it to full reports (as SDL does).
                    let mut feat = [0u8; 41];
                    feat[0] = 0x05;
                    let _ = dev.get_feature_report(&mut feat);
                }
                self.seq = 0;
                self.force = true; // push a known state right away
            }
        }

        fn send(&mut self, heavy: u8, light: u8, mode: TriggerMode, rumble_bits: bool) -> bool {
            let Some(dev) = self.dev.as_ref() else { return false };
            let mut buf = [0u8; 78];
            let (c, len) = if self.bluetooth {
                buf[0] = 0x31;
                buf[1] = self.seq << 4;
                self.seq = (self.seq + 1) & 0x0F;
                buf[2] = 0x10;
                (3, 78)
            } else {
                buf[0] = 0x02;
                (1, 63)
            };

            buf[c] = 0x04; // flags0: update right trigger effect
            if rumble_bits {
                buf[c] |= 0x01 | 0x02;      // emulated rumble + disable audio haptics
                buf[c + 2] = light >> 1;    // halved, as SDL does for the legacy rumble path
                buf[c + 3] = heavy >> 1;
            }
            match mode {
                TriggerMode::Resist => { buf[c + 10] = 0x01; buf[c + 12] = 110; } // SDL's "constant resistance"
                TriggerMode::Off => { buf[c + 10] = 0x05; }                         // SDL's "clear trigger effect"
            }

            if self.bluetooth {
                let crc = crc32(crc32(0, &[0xA2]), &buf[..len - 4]);
                buf[len - 4..len].copy_from_slice(&crc.to_le_bytes());
            }
            dev.write(&buf[..len]).is_ok()
        }

        pub fn tick(&mut self, dt: f32, heavy: u8, light: u8, mode: TriggerMode) {
            if self.dev.is_none() {
                self.retry -= dt;
                if self.retry > 0.0 { return; }
                self.retry = 2.0; // look for a pad every couple of seconds
                self.open();
                if self.dev.is_none() { return; }
            }
            self.since_send += dt;

            let (lh, ll, lm) = self.last;
            let changed = heavy != lh || light != ll || mode != lm || self.force;
            let active = heavy > 0 || light > 0 || mode != TriggerMode::Off;
            if !changed && !(active && self.since_send >= 0.5) { return; } // write on change, plus a keep-alive

            let rumble_bits = heavy > 0 || light > 0 || lh > 0 || ll > 0; // one last write with zeros stops the motors
            if !self.send(heavy, light, mode, rumble_bits) {
                self.dev = None;
                self.last = (0, 0, TriggerMode::Off);
                return;
            }
            self.last = (heavy, light, mode);
            self.since_send = 0.0;
            self.force = false;
        }

        pub fn shutdown(&mut self) {
            if self.dev.is_some() {
                self.send(0, 0, TriggerMode::Off, true);
                self.dev = None;
            }
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
mod backend {
    use super::{PadSnapshot, TriggerMode};
    pub fn diagnose() -> Vec<String> { vec!["  (HID not supported on this OS)".into()] }
    pub struct Backend;
    impl Backend {
        pub fn read_pad(&mut self) -> Option<PadSnapshot> { None }
        pub fn new() -> Self { Backend }
        pub fn connected(&self) -> bool { false }
        pub fn status(&self) -> String { "not supported on this OS".into() }
        pub fn tick(&mut self, _dt: f32, _h: u8, _l: u8, _m: TriggerMode) {}
        pub fn shutdown(&mut self) {}
    }
}

#[cfg(all(test, any(target_os = "windows", target_os = "linux")))]
mod tests {
    #[test]
    fn crc32_matches_standard_check_value() {
        // CRC-32/ISO-HDLC check value for "123456789"
        assert_eq!(super::backend::crc32(0, b"123456789"), 0xCBF4_3926);
    }
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn parses_usb_and_bluetooth_reports() {
        let mut usb = [0u8; 64];
        usb[0] = 0x01; usb[1] = 255; usb[2] = 0; // left stick full right, full up
        usb[8] = 0x20 | 0x02; // cross + hat east
        usb[9] = 0x20;        // options
        let p = parse_report(&usb).unwrap();
        assert!(p.lx > 0.99 && p.ly > 0.99);
        assert!(p.down.contains(&Button::South) && p.down.contains(&Button::DPadRight) && p.down.contains(&Button::Start));
        assert!(!p.down.contains(&Button::DPadUp));

        let mut bt = [0u8; 78];
        bt[0] = 0x31; bt[2] = 128; bt[9] = 0x08; bt[10] = 0x08; // hat released, R2
        let p = parse_report(&bt).unwrap();
        assert!(p.lx.abs() < 0.01 && p.down == vec![Button::RightTrigger2]);
    }
}
