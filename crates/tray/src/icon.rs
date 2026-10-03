//! The icon, drawn rather than shipped.
//!
//! A handful of RGBA pixels generated at startup, because the alternative is
//! carrying PNG files and a decoder for an image sixteen pixels across.
//!
//! It is the mark (decision 0048): a rounded tile, a ring for your space, a
//! stem that makes it a q, and a point of Qurb light inside — the same drawing
//! as the window's and `packaging/qurb.svg`, in the same 32-unit coordinates,
//! so the panel and the applications menu agree. What changes with the state
//! is the tile's colour and whether the ring is whole. At this size an icon is
//! read as a silhouette and a colour, not as a picture.

/// Icons are square; this is the side length.
const SIZE: u32 = 32;

/// Samples per pixel along each axis. Sixteen samples a pixel is what makes
/// the ring and the tile's corners look drawn rather than stepped.
const GRID: u32 = 4;

/// The drawing, in the mark's own coordinates: a 32 × 32 box.
const TILE: (f32, f32, f32) = (1.0, 31.0, 9.0); // from, to, corner radius
const RING: (f32, f32, f32) = (14.4, 6.2, 1.3); // centre (x = y), radius, half the stroke
const STEM: (f32, f32, f32) = (20.6, 11.0, 24.5); // x, top, bottom
const LIGHT: f32 = 1.9;
const WHITE: [f32; 3] = [255.0, 255.0, 255.0];
const QURB_LIGHT: [f32; 3] = [0xBD as f32, 0xE7 as f32, 0xD6 as f32];

/// What the icon is saying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// The mark as it is: everything is where it should be.
    Settled,
    /// A gap in the ring: something is moving.
    Working,
    /// Grey: running, but nothing else is reachable.
    Alone,
    /// A gap in the ring, on a warning colour.
    Problem,
}

impl From<qurb_cli::status::State> for Look {
    fn from(state: qurb_cli::status::State) -> Self {
        use qurb_cli::status::State::*;
        match state {
            Starting | Working => Look::Working,
            UpToDate => Look::Settled,
            Alone => Look::Alone,
            Problem => Look::Problem,
        }
    }
}

impl Look {
    /// The tile's gradient, top left to bottom right, as RGB.
    ///
    /// Each is dark enough for the white glyph to read on it and light enough
    /// to stand off a dark panel: a tray icon is not told which it sits on.
    /// Qurb green for the mark itself, the direction's neutral and error
    /// colours for the other two.
    fn tile(&self) -> ([u8; 3], [u8; 3]) {
        match self {
            Look::Settled | Look::Working => ([0x3C, 0x80, 0x6A], [0x22, 0x50, 0x3F]),
            Look::Alone => ([0x8A, 0x8D, 0x86], [0x5F, 0x62, 0x5C]),
            Look::Problem => ([0xBA, 0x55, 0x4B], [0x86, 0x33, 0x2D]),
        }
    }

    fn ring_is_open(&self) -> bool {
        matches!(self, Look::Working | Look::Problem)
    }
}

/// Render an icon: RGBA, `SIZE` × `SIZE`, ready for `tray-icon`.
pub fn render(look: Look) -> (Vec<u8>, u32, u32) {
    let mut pixels = vec![0u8; (SIZE * SIZE * 4) as usize];
    let scale = 32.0 / SIZE as f32;
    let samples = (GRID * GRID) as f32;

    for y in 0..SIZE {
        for x in 0..SIZE {
            // Premultiplied sums over the pixel's samples, so an edge blends
            // into transparency rather than into black.
            let mut sum = [0.0f32; 4];
            for sy in 0..GRID {
                for sx in 0..GRID {
                    let px = (x as f32 + (sx as f32 + 0.5) / GRID as f32) * scale;
                    let py = (y as f32 + (sy as f32 + 0.5) / GRID as f32) * scale;
                    if let Some(rgb) = colour_at(look, px, py) {
                        for c in 0..3 {
                            sum[c] += rgb[c];
                        }
                        sum[3] += 1.0;
                    }
                }
            }
            if sum[3] > 0.0 {
                let i = ((y * SIZE + x) * 4) as usize;
                for c in 0..3 {
                    pixels[i + c] = (sum[c] / sum[3]).round() as u8;
                }
                pixels[i + 3] = (sum[3] / samples * 255.0).round() as u8;
            }
        }
    }

    (pixels, SIZE, SIZE)
}

/// The colour at one point of the mark, or `None` outside the tile.
fn colour_at(look: Look, x: f32, y: f32) -> Option<[f32; 3]> {
    if !inside_tile(x, y) {
        return None;
    }
    let (cx, radius, half) = RING;
    let from_centre = ((x - cx).powi(2) + (y - cx).powi(2)).sqrt();

    let on_ring = (from_centre - radius).abs() <= half && !(look.ring_is_open() && in_gap(x - cx, y - cx));
    let (stem_x, top, bottom) = STEM;
    let on_stem = (x - stem_x).abs() <= half && (top..=bottom).contains(&y)
        || distance(x, y, stem_x, top) <= half
        || distance(x, y, stem_x, bottom) <= half;

    if on_ring || on_stem {
        return Some(WHITE);
    }
    if from_centre <= LIGHT {
        return Some(QURB_LIGHT);
    }

    // Along the diagonal, as the SVG's gradient runs.
    let (from, to, _) = TILE;
    let t = (((x - from) + (y - from)) / (2.0 * (to - from))).clamp(0.0, 1.0);
    let (start, end) = look.tile();
    Some(std::array::from_fn(|c| start[c] as f32 + (end[c] as f32 - start[c] as f32) * t))
}

/// Within the rounded square, corners included.
fn inside_tile(x: f32, y: f32) -> bool {
    let (from, to, r) = TILE;
    if !(from..=to).contains(&x) || !(from..=to).contains(&y) {
        return false;
    }
    // Only the four corner squares need the circle test.
    let cx = x.clamp(from + r, to - r);
    let cy = y.clamp(from + r, to - r);
    distance(x, y, cx, cy) <= r
}

/// The gap in the ring, for the looks that have one: the lower left, from
/// about seven o'clock to nine, where the stem does not cover it. Reads as
/// "in motion" without animating — a tray icon that animates is a tray icon
/// people turn off.
fn in_gap(dx: f32, dy: f32) -> bool {
    // y grows downwards, so a positive angle is below the centre.
    (1.95..3.0).contains(&dy.atan2(dx))
}

fn distance(x: f32, y: f32, to_x: f32, to_y: f32) -> f32 {
    ((x - to_x).powi(2) + (y - to_y).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
        let i = ((y * SIZE + x) * 4) as usize;
        [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
    }

    #[test]
    fn an_icon_is_the_size_it_claims() {
        let (pixels, w, h) = render(Look::Settled);
        assert_eq!(w, SIZE);
        assert_eq!(h, SIZE);
        assert_eq!(pixels.len(), (SIZE * SIZE * 4) as usize);
    }

    /// Every look must actually draw something. An icon that renders empty is
    /// invisible in the tray, which is indistinguishable from the program not
    /// running — the one thing this must never look like.
    #[test]
    fn every_look_draws_visible_pixels() {
        for look in [Look::Settled, Look::Working, Look::Alone, Look::Problem] {
            let (pixels, _, _) = render(look);
            let lit = pixels.chunks(4).filter(|p| p[3] > 0).count();
            assert!(lit > 50, "{look:?} drew only {lit} visible pixels");
        }
    }

    /// The looks must be distinguishable, or the icon says nothing.
    #[test]
    fn the_looks_differ_from_each_other() {
        let settled = render(Look::Settled).0;
        for look in [Look::Working, Look::Alone, Look::Problem] {
            assert_ne!(render(look).0, settled, "{look:?} looks identical to Settled");
        }
    }

    /// It is the mark: a tile with rounded corners, white where the ring and
    /// the stem are, the light at the centre — and a gap only while working.
    #[test]
    fn it_draws_the_mark() {
        let (settled, _, _) = render(Look::Settled);
        assert_eq!(pixel(&settled, 0, 0)[3], 0, "a corner outside the rounded tile");
        assert_eq!(pixel(&settled, 16, 2)[3], 255, "the tile's top edge, inside");
        assert_eq!(pixel(&settled, 14, 14)[..3], [0xBD, 0xE7, 0xD6], "the light at the centre");
        assert_eq!(pixel(&settled, 20, 20)[..3], [255, 255, 255], "the stem");
        assert_eq!(pixel(&settled, 8, 14)[..3], [255, 255, 255], "the ring, on its left");

        // The lower left of the ring is where the gap is.
        let (working, _, _) = render(Look::Working);
        assert_eq!(pixel(&settled, 9, 17)[..3], [255, 255, 255], "the ring at eight o'clock");
        assert_ne!(pixel(&working, 9, 17)[..3], [255, 255, 255], "no gap while working");
    }
}
