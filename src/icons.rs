//! Icons drawn in code (no image files): each shape is a distance function, anti-aliased by its
//! coverage of the pixel. 32×32, one shade with alpha; macOS uses them as templates.

use tray_icon::Icon;
use tray_icon::menu::Icon as MenuIcon;

const SIZE: u32 = 32;

/// macOS recolours template icons; elsewhere the tray is usually dark and menus light.
#[cfg(target_os = "macos")]
const TRAY_SHADE: u8 = 0;
#[cfg(not(target_os = "macos"))]
const TRAY_SHADE: u8 = 255;
#[cfg(target_os = "macos")]
const MENU_SHADE: u8 = 0;
#[cfg(not(target_os = "macos"))]
const MENU_SHADE: u8 = 110;

/// Status bar: a Y in a ring when active; crossed out like a no-parking sign when paused.
pub fn tray(active: bool) -> Icon {
    Icon::from_rgba(tray_pixels(active), SIZE, SIZE).expect("32×32 RGBA is a valid icon")
}

fn tray_pixels(active: bool) -> Vec<u8> {
    render(TRAY_SHADE, |x, y| {
        let ring = stroke(((x - 16.0).hypot(y - 16.0) - 13.5).abs(), 1.7);
        let letter = stroke(
            segment(x, y, (11.0, 7.5), (16.0, 15.0))
                .min(segment(x, y, (21.0, 7.5), (16.0, 15.0)))
                .min(segment(x, y, (16.0, 15.0), (16.0, 24.5))),
            2.4,
        );
        if active {
            return ring.max(letter);
        }
        let slash_distance = segment(x, y, (6.5, 6.5), (25.5, 25.5));
        let gap = (slash_distance - 1.9).clamp(0.0, 1.0);
        ring.max(letter * gap).max(stroke(slash_distance, 1.9))
    })
}

pub fn pause() -> MenuIcon {
    menu_icon(render(MENU_SHADE, |x, y| {
        stroke(segment(x, y, (11.5, 8.5), (11.5, 23.5)).min(segment(x, y, (20.5, 8.5), (20.5, 23.5))), 3.0)
    }))
}

pub fn play() -> MenuIcon {
    let corners = [(10.0, 7.5), (25.0, 16.0), (10.0, 24.5)];
    menu_icon(render(MENU_SHADE, move |x, y| {
        let edge = (0..3).map(|i| segment(x, y, corners[i], corners[(i + 1) % 3])).fold(f32::MAX, f32::min);
        if inside(x, y, &corners) { 1.0 } else { (0.5 - edge).clamp(0.0, 1.0) }
    }))
}

pub fn gear() -> MenuIcon {
    menu_icon(render(MENU_SHADE, |x, y| {
        let (dx, dy) = (x - 16.0, y - 16.0);
        let distance = dx.hypot(dy);
        let teeth = (dy.atan2(dx) * 8.0).cos().clamp(-0.35, 0.35) / 0.35;
        let outer = 10.5 + 2.6 * (teeth + 1.0) / 2.0;
        (outer - distance + 0.5).clamp(0.0, 1.0) * (distance - 4.0 + 0.5).clamp(0.0, 1.0)
    }))
}

fn menu_icon(rgba: Vec<u8>) -> MenuIcon {
    MenuIcon::from_rgba(rgba, SIZE, SIZE).expect("32×32 RGBA is a valid icon")
}

/// RGBA pixels of one shade, the alpha given by `coverage` at each pixel centre.
/// The Dock and window icon while yesyesyes runs.
pub fn app() -> eframe::egui::IconData {
    eframe::egui::IconData { rgba: app_pixels(256), width: 256, height: 256 }
}

/// The app icon, `size`×`size` RGBA: the status bar Y in black on an opaque white square that
/// fills the image, so macOS rounds it like its own icons instead of framing it in grey.
pub fn app_pixels(size: u32) -> Vec<u8> {
    let n = size as f32;
    let center = n / 2.0;
    // Pixels per unit of the 32×32 drawing of the Y.
    let scale = n / SIZE as f32 * 0.6;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for row in 0..size {
        for column in 0..size {
            let (x, y) = (column as f32 + 0.5, row as f32 + 0.5);
            let (gx, gy) = (16.0 + (x - center) / scale, 16.0 + (y - center) / scale);
            let ring = ((gx - 16.0).hypot(gy - 16.0) - 13.5).abs();
            let letter = segment(gx, gy, (11.0, 7.5), (16.0, 15.0))
                .min(segment(gx, gy, (21.0, 7.5), (16.0, 15.0)))
                .min(segment(gx, gy, (16.0, 15.0), (16.0, 24.5)));
            let ink = stroke(ring * scale, 1.7 * scale).max(stroke(letter * scale, 2.4 * scale));
            let shade = (255.0 * (1.0 - ink)).round() as u8;
            rgba.extend_from_slice(&[shade, shade, shade, 255]);
        }
    }
    rgba
}

/// The sizes of a macOS iconset: file name and side in pixels.
pub const ICONSET: [(&str, u32); 10] = [
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
];

/// Writes the app icon at every iconset size into `dir`, for `iconutil` (scripts/build-macos-app.sh).
pub fn write_iconset(dir: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for (name, size) in ICONSET {
        std::fs::write(dir.join(name), png(&app_pixels(size), size, size))?;
    }
    Ok(())
}

/// A PNG of RGBA pixels, uncompressed (zlib stored blocks): no image library needed.
pub fn png(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let mut raw = Vec::with_capacity(rgba.len() + height as usize);
    for line in rgba.chunks(width as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(line);
    }
    let mut zlib = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    for (n, block) in blocks.iter().enumerate() {
        zlib.push(u8::from(n + 1 == blocks.len()));
        let length = block.len() as u16;
        zlib.extend_from_slice(&length.to_le_bytes());
        zlib.extend_from_slice(&(!length).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut out = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];
    for (kind, data) in [(b"IHDR", &header[..]), (b"IDAT", &zlib[..]), (b"IEND", &[][..])] {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = out.len();
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        let crc = crc32(&out[start..]);
        out.extend_from_slice(&crc.to_be_bytes());
    }
    out
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in bytes {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn render(shade: u8, coverage: impl Fn(f32, f32) -> f32) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for row in 0..SIZE {
        for column in 0..SIZE {
            let alpha = coverage(column as f32 + 0.5, row as f32 + 0.5).clamp(0.0, 1.0);
            rgba.extend_from_slice(&[shade, shade, shade, (alpha * 255.0).round() as u8]);
        }
    }
    rgba
}

/// Coverage of a line of the given width at `distance` from its centre.
fn stroke(distance: f32, width: f32) -> f32 {
    (width / 2.0 - distance + 0.5).clamp(0.0, 1.0)
}

fn segment(x: f32, y: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = (((x - a.0) * dx + (y - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    (x - a.0 - t * dx).hypot(y - a.1 - t * dy)
}

fn inside(x: f32, y: f32, corners: &[(f32, f32); 3]) -> bool {
    let side = |a: (f32, f32), b: (f32, f32)| (b.0 - a.0) * (y - a.1) - (b.1 - a.1) * (x - a.0);
    let signs = [side(corners[0], corners[1]), side(corners[1], corners[2]), side(corners[2], corners[0])];
    signs.iter().all(|s| *s >= 0.0) || signs.iter().all(|s| *s <= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_of_the_png_writer() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn writes_a_png() {
        let pixels = app_pixels(300);
        let png = png(&pixels, 300, 300);
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(u32::from_be_bytes(png[16..20].try_into().unwrap()), 300);
        assert!(png.ends_with(&[0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]));
        assert!(png.len() > pixels.len(), "stored, not compressed: several zlib blocks");
    }

    #[test]
    fn the_app_icon_is_a_black_y_on_white() {
        let size = 64;
        let pixels = app_pixels(size);
        let at = |x: u32, y: u32| &pixels[((y * size + x) * 4) as usize..((y * size + x) * 4 + 4) as usize];
        assert_eq!(at(0, 0), [255, 255, 255, 255], "opaque white to the edge");
        assert_eq!(at(32, 40), [0, 0, 0, 255], "the stem of the Y");
        assert_eq!(at(10, 32), [255, 255, 255, 255], "white between the ring and the edge");
        assert_eq!(ICONSET.len(), 10);
    }

    fn alpha(pixels: &[u8], x: u32, y: u32) -> u8 {
        pixels[((y * SIZE + x) * 4 + 3) as usize]
    }

    #[test]
    fn paused_icon_is_crossed_out() {
        let (active, paused) = (tray_pixels(true), tray_pixels(false));
        assert_eq!(alpha(&active, 16, 21), 255, "stem of the Y");
        assert_eq!(alpha(&active, 8, 8), 0);
        assert_eq!(alpha(&paused, 8, 8), 255, "the slash");
        assert_eq!(alpha(&paused, 16, 16), 255, "the slash crosses the centre");
    }

    #[test]
    fn menu_icons_build() {
        let _ = (pause(), play(), gear());
    }
}

