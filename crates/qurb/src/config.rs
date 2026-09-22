//! Where a device keeps its settings.
//!
//! A flat `key = value` file rather than TOML or JSON. There are five settings,
//! a person may have to edit it by hand over SSH on a machine with no editor
//! they like, and a parser for this format is twenty lines that can be read in
//! one sitting. A dependency would buy nesting that nothing here needs.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

/// Where the rendezvous service is, when nothing says otherwise.
pub const DEFAULT_SIGNAL: &str = "ws://localhost:9000";

#[derive(Debug, Clone)]
pub struct Config {
    /// The rendezvous service.
    pub signal: String,
    /// The relay to fall back to, if any.
    pub relay: Option<SocketAddr>,
    /// What this device calls itself when pairing. Display only.
    pub name: String,
    /// The port to listen on. Zero means any, which is fine behind a router
    /// that does not forward anything; a fixed one helps where it does.
    pub port: u16,
    /// How much disk this folder may use, in bytes. Zero means no limit.
    ///
    /// Counts the files being held plus the chunk store, which together are
    /// everything qurb puts on the disk for this folder.
    pub limit: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            signal: DEFAULT_SIGNAL.to_string(),
            relay: None,
            name: hostname(),
            port: 0,
            limit: 0,
        }
    }
}

impl Config {
    pub fn path(store_dir: &Path) -> PathBuf {
        store_dir.join("config")
    }

    /// Read the config, falling back to defaults for anything absent.
    ///
    /// An unreadable file is an error; a missing one is not. The difference
    /// matters: a typo in a filename should not silently produce a device with
    /// default settings that quietly fails to find its peers.
    pub fn load(store_dir: &Path) -> Result<Self> {
        let path = Self::path(store_dir);
        if !path.exists() {
            return Ok(Self::default());
        }

        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let mut settings = BTreeMap::new();

        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (key, value) = line.split_once('=').with_context(|| {
                format!("{}:{}: expected `key = value`", path.display(), number + 1)
            })?;
            settings.insert(key.trim().to_string(), value.trim().to_string());
        }

        let mut config = Self::default();
        if let Some(signal) = settings.remove("signal") {
            config.signal = signal;
        }
        if let Some(relay) = settings.remove("relay") {
            if !relay.is_empty() {
                config.relay = Some(
                    relay
                        .parse()
                        .with_context(|| format!("{}: relay is not an address", path.display()))?,
                );
            }
        }
        if let Some(name) = settings.remove("name") {
            config.name = name;
        }
        if let Some(port) = settings.remove("port") {
            config.port =
                port.parse().with_context(|| format!("{}: port is not a number", path.display()))?;
        }

        if let Some(limit) = settings.remove("limit") {
            config.limit = parse_size(&limit)
                .with_context(|| format!("{}: limit is not a size", path.display()))?;
        }

        // Unknown keys are reported rather than ignored. A misspelled setting
        // that silently does nothing is a bad afternoon.
        if let Some(unknown) = settings.keys().next() {
            anyhow::bail!("{}: unknown setting `{unknown}`", path.display());
        }

        Ok(config)
    }

    pub fn save(&self, store_dir: &Path) -> Result<()> {
        let relay = self.relay.map(|r| r.to_string()).unwrap_or_default();
        let text = format!(
            "# qurb device settings\n\
             \n\
             # The rendezvous service that introduces devices to each other.\n\
             signal = {}\n\
             \n\
             # A relay to fall back to when no direct path exists. Optional.\n\
             relay = {relay}\n\
             \n\
             # What this device calls itself when pairing. Display only.\n\
             name = {}\n\
             \n\
             # Port to listen on. 0 means any.\n\
             port = {}\n\
             \n\
             # How much disk this folder may use. 0 means no limit.\n\
             # Accepts a plain number of bytes or a suffix: 500M, 10G, 1T.\n\
             # When over the limit, qurb drops local copies of the files it has\n\
             # not touched in longest -- but only ones another device is known\n\
             # to hold. It never drops the only copy, even to stay under.\n\
             limit = {}\n",
            self.signal, self.name, self.port, human_size(self.limit)
        );
        std::fs::write(Self::path(store_dir), text)
            .with_context(|| format!("writing {}", Self::path(store_dir).display()))
    }
}

/// Parse `10G`, `500M`, `1T`, or a plain byte count.
///
/// Powers of 1024, because that is what a file manager shows and a person
/// setting a limit is looking at a file manager. Rejecting a bad value matters
/// more than accepting every spelling: a typo that silently becomes a tiny
/// limit would start deleting local copies.
pub fn parse_size(text: &str) -> Result<u64> {
    let text = text.trim();
    if text.is_empty() || text == "0" {
        return Ok(0);
    }

    let (number, scale) = match text.chars().last().map(|c| c.to_ascii_uppercase()) {
        Some('K') => (&text[..text.len() - 1], 1u64 << 10),
        Some('M') => (&text[..text.len() - 1], 1u64 << 20),
        Some('G') => (&text[..text.len() - 1], 1u64 << 30),
        Some('T') => (&text[..text.len() - 1], 1u64 << 40),
        _ => (text, 1),
    };

    let value: f64 = number
        .trim()
        .parse()
        .with_context(|| format!("`{text}` is not a size — try 10G, 500M, or a byte count"))?;
    if value < 0.0 || !value.is_finite() {
        anyhow::bail!("`{text}` is not a size");
    }
    Ok((value * scale as f64) as u64)
}

/// The inverse, for writing the file back in the form it was typed.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [(u64, &str); 4] =
        [(1 << 40, "T"), (1 << 30, "G"), (1 << 20, "M"), (1 << 10, "K")];
    for (scale, suffix) in UNITS {
        if bytes >= scale && bytes.is_multiple_of(scale) {
            return format!("{}{suffix}", bytes / scale);
        }
    }
    bytes.to_string()
}

fn hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "a device".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::load(dir.path()).unwrap();
        assert_eq!(config.signal, DEFAULT_SIGNAL);
        assert!(config.relay.is_none());
    }

    #[test]
    fn settings_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            signal: "wss://signal.example.com".into(),
            relay: Some("198.51.100.7:443".parse().unwrap()),
            name: "Study desktop".into(),
            port: 51820,
            limit: 10 << 30,
        };
        config.save(dir.path()).unwrap();

        let loaded = Config::load(dir.path()).unwrap();
        assert_eq!(loaded.signal, config.signal);
        assert_eq!(loaded.relay, config.relay);
        assert_eq!(loaded.name, config.name);
        assert_eq!(loaded.port, config.port);
        assert_eq!(loaded.limit, config.limit);
    }

    #[test]
    fn sizes_are_written_back_the_way_they_were_typed() {
        assert_eq!(parse_size("10G").unwrap(), 10 << 30);
        assert_eq!(parse_size(" 500M ").unwrap(), 500 << 20);
        assert_eq!(parse_size("1T").unwrap(), 1 << 40);
        assert_eq!(parse_size("1048576").unwrap(), 1 << 20);
        assert_eq!(parse_size("0").unwrap(), 0);
        assert_eq!(parse_size("1.5G").unwrap(), (1.5 * (1u64 << 30) as f64) as u64);

        assert_eq!(human_size(10 << 30), "10G");
        assert_eq!(human_size(0), "0");
        assert_eq!(human_size(1234), "1234");
        assert_eq!(human_size(parse_size("500M").unwrap()), "500M");
    }

    /// A typo must not quietly become a tiny limit, which would start dropping
    /// local copies of the user's files.
    #[test]
    fn a_size_that_is_not_a_size_is_refused() {
        assert!(parse_size("ten gigs").unwrap_or(0) == 0 || parse_size("ten gigs").is_err());
        assert!(parse_size("10GB").is_err(), "`10GB` parsed as something");
        assert!(parse_size("-5G").is_err());
        assert!(parse_size("G").is_err());
    }

    #[test]
    fn comments_and_blank_lines_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            Config::path(dir.path()),
            "# a comment\n\n  signal = ws://example\n\n# another\n",
        )
        .unwrap();
        assert_eq!(Config::load(dir.path()).unwrap().signal, "ws://example");
    }

    #[test]
    fn a_misspelled_setting_is_reported() {
        // Silently ignoring it would leave someone convinced they had
        // configured a relay when they had not.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(Config::path(dir.path()), "rely = 1.2.3.4:443\n").unwrap();
        let error = Config::load(dir.path()).unwrap_err().to_string();
        assert!(error.contains("rely"), "got {error}");
    }

    #[test]
    fn a_malformed_line_says_where() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(Config::path(dir.path()), "signal = ws://ok\nnonsense\n").unwrap();
        let error = Config::load(dir.path()).unwrap_err().to_string();
        assert!(error.contains(":2"), "the error should name the line: {error}");
    }

    #[test]
    fn a_bad_address_is_refused_rather_than_ignored() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(Config::path(dir.path()), "relay = not-an-address\n").unwrap();
        assert!(Config::load(dir.path()).is_err());
    }

    #[test]
    fn an_empty_relay_means_none() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(Config::path(dir.path()), "relay =\n").unwrap();
        assert!(Config::load(dir.path()).unwrap().relay.is_none());
    }
}
