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
}
