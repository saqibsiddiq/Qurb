//! Write a pairing QR code to a file, to be scanned by a real camera.
//!
//! The unit tests check that the SVG encodes the same matrix the terminal
//! renderer draws, which is as far as a test can go: whether a phone can
//! actually read it off a screen is a question about optics, and the only way
//! to answer it is to point a phone at one.
//!
//! ```bash
//! cargo run --release -p qurb-cli --example qrcheck > /tmp/pairing.svg
//! ```
//!
//! Then open it and scan it. It should decode to the string below, which is a
//! real pairing code's shape and an expired one's contents — it pairs with
//! nothing.

fn main() {
    const SAMPLE: &str = "qurb1-IEGKYVP65H6QAGVHZ2LDXEHLOGFNGMPDYGYC5JOCPIDAQSBGIBFOQCXAJEM67TLJIRNMDN6Q6YN7QHZCVRVAAAAAAACMBKABAQGN2";

    match qurb_cli::qr::svg(SAMPLE) {
        Ok(drawn) => {
            eprintln!("scan this; it should read back as:\n{SAMPLE}");
            print!("{drawn}");
        }
        Err(e) => {
            eprintln!("could not render: {e}");
            std::process::exit(1);
        }
    }
}
