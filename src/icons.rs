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

