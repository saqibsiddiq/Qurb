//! The icon, drawn rather than shipped.
//!
//! A handful of RGBA pixels generated at startup, because the alternative is
//! carrying PNG files and a decoder for an image sixteen pixels across. The
//! shapes are deliberately crude — at this size a tray icon is read as a
//! silhouette and a colour, not as a picture.

/// Icons are square; this is the side length.
const SIZE: u32 = 32;

/// What the icon is saying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// A closed ring: everything is where it should be.
    Settled,
    /// A ring with a gap: something is moving.
    Working,
    /// Dimmed: running, but nothing else is reachable.
    Alone,
    /// A ring with a bite out of it, in a warning colour.
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
    /// Foreground colour, as RGB.
    ///
    /// Chosen to be legible on both a light and a dark panel, which rules out
    /// anything very pale or very dark: a tray icon does not get told which it
    /// is sitting on.
    fn colour(&self) -> [u8; 3] {
        match self {
            Look::Settled => [0x2B, 0x5C, 0xE6],
            Look::Working => [0x2B, 0x5C, 0xE6],
            Look::Alone => [0x8A, 0x8A, 0x92],
            Look::Problem => [0xD9, 0x4B, 0x2B],
        }
    }
}

/// Render an icon: RGBA, `SIZE` × `SIZE`, ready for `tray-icon`.
pub fn render(look: Look) -> (Vec<u8>, u32, u32) {
    let [r, g, b] = look.colour();
    let mut pixels = vec![0u8; (SIZE * SIZE * 4) as usize];

    let centre = (SIZE as f32 - 1.0) / 2.0;
    let outer = SIZE as f32 * 0.46;
    let inner = SIZE as f32 * 0.28;

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - centre;
            let dy = y as f32 - centre;
            let distance = (dx * dx + dy * dy).sqrt();

            // Antialiased by coverage over one pixel, which is the difference
            // between a ring that looks drawn and one that looks pixelated.
            let coverage = ring_coverage(distance, inner, outer)
                * gap_coverage(look, dx, dy);

            if coverage > 0.0 {
                let i = ((y * SIZE + x) * 4) as usize;
                pixels[i] = r;
                pixels[i + 1] = g;
                pixels[i + 2] = b;
                pixels[i + 3] = (coverage.clamp(0.0, 1.0) * 255.0) as u8;
            }
        }
    }

    (pixels, SIZE, SIZE)
}

/// How much of a pixel at `distance` falls inside the ring.
fn ring_coverage(distance: f32, inner: f32, outer: f32) -> f32 {
    let outside = (outer - distance).clamp(0.0, 1.0);
    let inside = (distance - inner).clamp(0.0, 1.0);
    outside * inside
}

/// The gap, for the looks that have one.
///
/// A wedge removed from the upper right, which reads as "in motion" at a glance
/// without needing animation — a tray icon that animates is a tray icon people
/// turn off.
fn gap_coverage(look: Look, dx: f32, dy: f32) -> f32 {
    match look {
        Look::Settled | Look::Alone => 1.0,
        Look::Working | Look::Problem => {
            let angle = dy.atan2(dx);
            // Roughly the 1-to-2-o'clock sector.
            let in_gap = (-1.2..-0.35).contains(&angle);
            if in_gap {
                0.0
            } else {
                1.0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
