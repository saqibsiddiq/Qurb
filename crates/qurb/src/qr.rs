//! Pairing codes as QR codes.
//!
//! The code carries the inviter's full fingerprint and has to travel *outside*
//! the network being paired — someone who can change what you see has already
//! won, so sending it over the link it authorises would defeat the point. A QR
//! code on a screen, read by a camera, is exactly that: the bytes cross the
//! room as light, not as packets.
//!
//! It is also the only form of a 107-character code that a person will use
//! twice. Typing one is possible and reading one aloud works; neither is
//! something anybody does willingly.

use qrcode::{EcLevel, QrCode};

/// Render `text` as a QR code for a terminal.
///
/// Drawn with half-block characters, two rows of the code per line of text.
/// A terminal cell is about twice as tall as it is wide, so one character per
/// module would produce a code stretched vertically — which cameras read
/// badly — and this makes the result close to square.
///
/// A quiet zone is included. The specification requires four modules of blank
/// margin, and scanners genuinely fail without it: the code is found by
/// looking for its finder patterns against a clear background, and a terminal
/// prompt printed flush against the edge is not one.
pub fn terminal(text: &str) -> Result<String, qrcode::types::QrError> {
    // Low correction, because the code is read off a screen rather than
    // something creased or smudged, and lower correction means a smaller
    // symbol -- which matters when the display is a text terminal.
    let code = QrCode::with_error_correction_level(text, EcLevel::L)?;
    let modules = code.to_colors();
    let width = code.width();

    const QUIET: usize = 4;
    let side = width + QUIET * 2;
    let dark = |x: usize, y: usize| -> bool {
        if x < QUIET || y < QUIET || x >= width + QUIET || y >= width + QUIET {
            return false;
        }
        modules[(y - QUIET) * width + (x - QUIET)] == qrcode::Color::Dark
    };

    let mut out = String::with_capacity(side * side / 2);
    // Two rows at a time: the upper half of a cell and the lower half.
    for y in (0..side).step_by(2) {
        for x in 0..side {
            let upper = dark(x, y);
            let lower = if y + 1 < side { dark(x, y + 1) } else { false };
            // Dark modules are drawn as *light* blocks, because a terminal is
            // usually light-on-dark and a scanner needs the finder patterns to
            // be the darker of the two. Inverting here means the code reads
            // correctly on the terminals people actually have.
            out.push(match (upper, lower) {
                (true, true) => '█',
                (true, false) => '▀',
                (false, true) => '▄',
                (false, false) => ' ',
            });
        }
        out.push('\n');
    }
    Ok(out)
}

/// Render `text` as a QR code in SVG, sized in module units.
///
/// One `<path>` of filled squares on a white ground, with a `viewBox` in
/// modules so the caller decides how big it is on screen. No width or height
/// attributes for the same reason: a pairing code is read by a camera held at
/// whatever distance is comfortable, and the display knows more about that than
/// this does.
///
/// White explicitly rather than transparently, whatever colour scheme is in
/// use. A scanner looks for dark modules against a light ground; a code drawn
/// dark-on-dark because the desktop is in dark mode is not a code.
pub fn svg(text: &str) -> Result<String, qrcode::types::QrError> {
    // Low correction, for the same reason as the terminal renderer: this is
    // read off a screen rather than off something creased, and lower correction
    // means a smaller symbol with larger modules at a given size.
    let code = QrCode::with_error_correction_level(text, EcLevel::L)?;
    let modules = code.to_colors();
    let width = code.width();

    // The specification requires four modules of blank margin, and scanners
    // genuinely fail without it: the code is found by looking for its finder
    // patterns against a clear background.
    const QUIET: usize = 4;
    let side = width + QUIET * 2;

    let mut path = String::with_capacity(width * width * 8);
    for y in 0..width {
        for x in 0..width {
            if modules[y * width + x] == qrcode::Color::Dark {
                // One `M` and one `h1v1h-1z` per module. Rectangles would be
                // clearer to read and produce one element each; a QR code is
                // upwards of a thousand modules, and a thousand elements is a
                // thousand things for a layout engine to think about.
                path.push_str(&format!("M{} {}h1v1h-1z", x + QUIET, y + QUIET));
            }
        }
    }

    Ok(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {side} {side}\" \
         shape-rendering=\"crispEdges\">\
         <rect width=\"{side}\" height=\"{side}\" fill=\"#ffffff\"/>\
         <path d=\"{path}\" fill=\"#000000\"/>\
         </svg>"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const INVITE: &str = "qurb1-IEGKYVP65H6QAGVHZ2LDXEHLOGFNGMPDYGYC5JOCPIDAQSBGIBFOQCXAJEM67TLJIRNMDN6Q6YN7QHZCVRVAAAAAAACMBKABAQGN2";

    /// A real pairing code must fit in a QR code at all. They are 107
    /// characters, which is well inside the format's capacity, but a change to
    /// the invite encoding could quietly push past it.
    #[test]
    fn a_real_pairing_code_encodes() {
        let rendered = terminal(INVITE).expect("a pairing code should fit in a QR code");
        assert!(!rendered.is_empty());
    }

    /// The quiet zone is not decoration. Scanners locate a code by its finder
    /// patterns against blank margin, and without it they simply do not see it.
    #[test]
    fn there_is_a_quiet_zone_on_every_side() {
        let rendered = terminal(INVITE).unwrap();
        let lines: Vec<&str> = rendered.lines().collect();

        // Two blank text rows, being four module rows.
        assert!(lines[0].trim().is_empty(), "no quiet zone above");
        assert!(lines[1].trim().is_empty(), "quiet zone above is too thin");
        assert!(lines[lines.len() - 1].trim().is_empty(), "no quiet zone below");

        for (i, line) in lines.iter().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            assert!(line.starts_with("    "), "no quiet zone to the left of line {i}");
            assert!(line.ends_with("    "), "no quiet zone to the right of line {i}");
        }
    }

    /// Half-blocks, so the result is roughly square in a terminal rather than
    /// twice as tall as it is wide.
    #[test]
    fn it_is_drawn_two_module_rows_per_line() {
        let rendered = terminal(INVITE).unwrap();
        let lines: Vec<&str> = rendered.lines().collect();
        let width = lines[0].chars().count();
        // Half as many lines as columns, give or take the odd row.
        assert!(
            lines.len() * 2 >= width - 1 && lines.len() * 2 <= width + 1,
            "{} lines for {width} columns is not square",
            lines.len()
        );
    }

    /// The SVG has to encode the same matrix the terminal renderer draws.
    /// They share `to_colors`, so if one scans the other does — and this is
    /// what catches a renderer that transposed or offset it.
    #[test]
    fn the_svg_draws_one_square_per_dark_module() {
        let drawn = svg(INVITE).unwrap();
        let code = QrCode::with_error_correction_level(INVITE, EcLevel::L).unwrap();
        let dark = code.to_colors().iter().filter(|c| **c == qrcode::Color::Dark).count();

        assert_eq!(drawn.matches("h1v1h-1z").count(), dark);
    }

    /// The same quiet zone as the terminal form, and for the same reason.
    #[test]
    fn the_svg_leaves_four_modules_of_margin() {
        let drawn = svg(INVITE).unwrap();
        let code = QrCode::with_error_correction_level(INVITE, EcLevel::L).unwrap();
        let side = code.width() + 8;

        assert!(drawn.contains(&format!("viewBox=\"0 0 {side} {side}\"")));
        // Nothing is drawn in the margin: every square starts at 4 or more.
        assert!(!drawn.contains("M0 "), "a module was drawn in the quiet zone");
        assert!(!drawn.contains(" 0h1v1h-1z"), "a module was drawn in the quiet zone");
    }

    /// Counting squares would not catch a renderer that transposed the matrix,
    /// because a transposed code has exactly as many dark modules as an upright
    /// one. This reads the positions back out and compares them cell by cell.
    #[test]
    fn every_square_is_where_the_matrix_says_it_is() {
        let drawn = svg(INVITE).unwrap();
        let code = QrCode::with_error_correction_level(INVITE, EcLevel::L).unwrap();
        let width = code.width();
        let modules = code.to_colors();

        // Each square is `M<x> <y>h1v1h-1z`, so the path reads back exactly.
        let mut found = std::collections::HashSet::new();
        for piece in drawn.split('M').skip(1) {
            let Some(coords) = piece.split('h').next() else { continue };
            let mut parts = coords.split(' ');
            let (Some(x), Some(y)) = (parts.next(), parts.next()) else { continue };
            let (Ok(x), Ok(y)) = (x.parse::<usize>(), y.parse::<usize>()) else { continue };
            found.insert((x - 4, y - 4));
        }

        for y in 0..width {
            for x in 0..width {
                let dark = modules[y * width + x] == qrcode::Color::Dark;
                assert_eq!(
                    found.contains(&(x, y)),
                    dark,
                    "module at ({x}, {y}) is drawn wrongly — is the matrix transposed?"
                );
            }
        }
    }

    /// Whatever colour scheme the desktop is in. A code drawn dark-on-dark is
    /// not a code, and the failure looks like a broken camera.
    #[test]
    fn the_svg_is_black_on_white_regardless() {
        let drawn = svg(INVITE).unwrap();
        assert!(drawn.contains("fill=\"#ffffff\""), "no light ground");
        assert!(drawn.contains("fill=\"#000000\""), "no dark modules");
    }
}
