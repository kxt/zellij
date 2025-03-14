//! Some general utility functions.

use std::{iter, str::from_utf8};

use strip_ansi_escapes::strip;

use colors_transform::{Color, Rgb};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::{fs, io};
use zellij_tile::data::{Palette, PaletteColor, PaletteSource, Theme};

const UNIX_PERMISSIONS: u32 = 0o700;

pub fn set_permissions(path: &Path) -> io::Result<()> {
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(UNIX_PERMISSIONS);
    fs::set_permissions(path, permissions)
}

fn ansi_len(s: &str) -> usize {
    from_utf8(&strip(s.as_bytes()).unwrap())
        .unwrap()
        .chars()
        .count()
}

pub fn adjust_to_size(s: &str, rows: usize, columns: usize) -> String {
    s.lines()
        .map(|l| {
            let actual_len = ansi_len(l);
            if actual_len > columns {
                let mut line = String::from(l);
                line.truncate(columns);
                line
            } else {
                [l, &str::repeat(" ", columns - ansi_len(l))].concat()
            }
        })
        .chain(iter::repeat(str::repeat(" ", columns)))
        .take(rows)
        .collect::<Vec<_>>()
        .join("\n\r")
}

// Colors
pub mod colors {
    pub const WHITE: u8 = 255;
    pub const GREEN: u8 = 154;
    pub const GRAY: u8 = 238;
    pub const BRIGHT_GRAY: u8 = 245;
    pub const RED: u8 = 88;
    pub const ORANGE: u8 = 166;
    pub const BLACK: u8 = 16;
}

pub fn _hex_to_rgb(hex: &str) -> (u8, u8, u8) {
    let rgb = Rgb::from_hex_str(hex)
        .expect("The passed argument must be a valid hex color")
        .as_tuple();
    (rgb.0 as u8, rgb.1 as u8, rgb.2 as u8)
}

pub fn default_palette() -> Palette {
    Palette {
        source: PaletteSource::Default,
        theme: Theme::Dark,
        fg: PaletteColor::EightBit(colors::BRIGHT_GRAY),
        bg: PaletteColor::EightBit(colors::GRAY),
        black: PaletteColor::EightBit(colors::BLACK),
        red: PaletteColor::EightBit(colors::RED),
        green: PaletteColor::EightBit(colors::GREEN),
        yellow: PaletteColor::EightBit(colors::GRAY),
        blue: PaletteColor::EightBit(colors::GRAY),
        magenta: PaletteColor::EightBit(colors::GRAY),
        cyan: PaletteColor::EightBit(colors::GRAY),
        white: PaletteColor::EightBit(colors::WHITE),
        orange: PaletteColor::EightBit(colors::ORANGE),
    }
}

// Dark magic
pub fn _detect_theme(bg: PaletteColor) -> Theme {
    match bg {
        PaletteColor::Rgb((r, g, b)) => {
            // HSP, P stands for perceived brightness
            let hsp: f64 = (0.299 * (r as f64 * r as f64)
                + 0.587 * (g as f64 * g as f64)
                + 0.114 * (b as f64 * b as f64))
                .sqrt();
            match hsp > 127.5 {
                true => Theme::Light,
                false => Theme::Dark,
            }
        }
        _ => Theme::Dark,
    }
}

// (this was shamelessly copied from alacritty)
//
// This returns the current terminal version as a unique number based on the
// semver version. The different versions are padded to ensure that a higher semver version will
// always report a higher version number.
pub fn version_number(mut version: &str) -> usize {
    if let Some(separator) = version.rfind('-') {
        version = &version[..separator];
    }

    let mut version_number = 0;

    let semver_versions = version.split('.');
    for (i, semver_version) in semver_versions.rev().enumerate() {
        let semver_number = semver_version.parse::<usize>().unwrap_or(0);
        version_number += usize::pow(100, i as u32) * semver_number;
    }

    version_number
}
