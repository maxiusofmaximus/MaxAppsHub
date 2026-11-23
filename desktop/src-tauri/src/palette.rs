//! The Hub's color palettes. They are Excubia's (the phone app): same names, colors and order.
//! Only "Sistema" is new: Windows' light or dark mode, painted with the Windows accent color.

use crate::i18n::tr;

/// Ink for letters on the app tiles, which keep their own accent color whatever the palette.
const INK: u32 = 0x0b1528;

/// The accent Sistema falls back to when Windows does not say which one it uses (Carbón's).
pub const FALLBACK_ACCENT: u32 = 0x7fa6d6;

/// The colors the Hub paints with. Every color in the window comes from here.
#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: u32,
    pub surface: u32,
    /// Excubia's "raised".
    pub surface_2: u32,
    pub surface_3: u32,
    pub border: u32,
    pub border_strong: u32,
    pub text: u32,
    pub text_secondary: u32,
    pub text_muted: u32,
    /// The accent: filled buttons and progress bars.
    pub primary: u32,
    /// The accent as text on the background, such as links.
    pub primary_text: u32,
    pub primary_soft: u32,
    /// Text on a filled button.
    pub on_primary: u32,
    /// Done and up to date.
    pub ok: u32,
    /// Blocked and failed.
    pub err: u32,
    /// Working and installing.
    pub info: u32,
    /// Idle and not checked.
    pub muted: u32,
    /// Excubia's shadows (ARGB). The Hub draws no shadows yet.
    #[allow(dead_code)]
    pub shadow_hard: u32,
    #[allow(dead_code)]
    pub shadow_dot: u32,
}

impl Palette {
    /// A status as a chip: its color as text on a faint tint of the surface.
    pub fn tone(&self, color: u32) -> (u32, u32) {
        (color, mix(self.surface, color, 0.15))
    }
}

const NOCHE: Palette = Palette {
    bg: 0x0b1426,
    surface: 0x101b33,
    surface_2: 0x17274a,
    surface_3: 0x1d2f55,
    border: 0x1f2e50,
    border_strong: 0x7f93bf,
    text: 0xefe6d2,
    text_secondary: 0xcfd8e8,
    text_muted: 0x9aa5bd,
    primary: 0xf2a93b,
    primary_text: 0xf2a93b,
    primary_soft: 0x3a2e17,
    on_primary: 0x1a1204,
    ok: 0x6fcf97,
    err: 0xff7a62,
    info: 0x8fb4e8,
    muted: 0x9aa5bd,
    shadow_hard: 0x8c000000,
    shadow_dot: 0x59000000,
};

const PERGAMINO: Palette = Palette {
    bg: 0xf6f0e1,
    surface: 0xfbf7ec,
    surface_2: 0xece2c8,
    surface_3: 0xe4d8bd,
    border: 0xe0d4b8,
    border_strong: 0x7a7265,
    text: 0x1c2438,
    text_secondary: 0x34405e,
    text_muted: 0x585f71,
    primary: 0xf2a93b,
    primary_text: 0x8a5208,
    primary_soft: 0xf6e4be,
    on_primary: 0x1a1204,
    ok: 0x276b4e,
    err: 0xa8321b,
    info: 0x2f5c9e,
    muted: 0x585f71,
    shadow_hard: 0x661c2438,
    shadow_dot: 0x291c2438,
};

/// Carbón: the dark base of Sistema when Windows is in dark mode.
const CARBON: Palette = Palette {
    bg: 0x181818,
    surface: 0x1f1f1f,
    surface_2: 0x2a2a2a,
    surface_3: 0x333333,
    border: 0x303030,
    border_strong: 0x8a8a8a,
    text: 0xececec,
    text_secondary: 0xcfcfcf,
    text_muted: 0xa3a3a3,
    primary: 0x7fa6d6,
    primary_text: 0x7fa6d6,
    primary_soft: 0x26313d,
    on_primary: 0x0d1622,
    ok: 0x7fd6a2,
    err: 0xff8a75,
    info: 0x8fb4e8,
    muted: 0xa3a3a3,
    shadow_hard: 0x8c000000,
    shadow_dot: 0x59000000,
};

/// Papel: the light base of Sistema when Windows is in light mode.
const PAPEL: Palette = Palette {
    bg: 0xfcfcfc,
    surface: 0xf3f3f3,
    surface_2: 0xe9e9e9,
    surface_3: 0xe0e0e0,
    border: 0xdcdcdc,
    border_strong: 0x6b6b6b,
    text: 0x141414,
    text_secondary: 0x333333,
    text_muted: 0x5a5a5a,
    primary: 0x2b6fb5,
    primary_text: 0x1f5a94,
    primary_soft: 0xdce8f6,
    on_primary: 0xffffff,
    ok: 0x276b4e,
    err: 0xa8321b,
    info: 0x2f5c9e,
    muted: 0x5a5a5a,
    shadow_hard: 0x661c2438,
    shadow_dot: 0x291c2438,
};

const MEDIANOCHE: Palette = Palette {
    bg: 0x00183a,
    surface: 0x001f4a,
    surface_2: 0x0a2c5e,
    surface_3: 0x12356b,
    border: 0x16396f,
    border_strong: 0x7e9ccd,
    text: 0xe8f0ff,
    text_secondary: 0xc7d6ee,
    text_muted: 0x93a8cc,
    primary: 0x6fb1ff,
    primary_text: 0x8fc4ff,
    primary_soft: 0x0e2a52,
    on_primary: 0x001226,
    ok: 0x7adfa8,
    err: 0xff8a7a,
    info: 0x8fc4ff,
    muted: 0x93a8cc,
    shadow_hard: 0x8c000000,
    shadow_dot: 0x59000000,
};

const CONTRASTE: Palette = Palette {
    bg: 0x000000,
    surface: 0x0a0a0a,
    surface_2: 0x1a1a1a,
    surface_3: 0x242424,
    border: 0xffffff,
    border_strong: 0xffffff,
    text: 0xffffff,
    text_secondary: 0xffffff,
    text_muted: 0xd0d0d0,
    primary: 0xffd400,
    primary_text: 0xffd400,
    primary_soft: 0x332b00,
    on_primary: 0x000000,
    ok: 0x6fe39b,
    err: 0xff6b5a,
    info: 0x7fc4ff,
    muted: 0xd0d0d0,
    shadow_hard: 0x8c000000,
    shadow_dot: 0x59000000,
};

const TELON: Palette = Palette {
    bg: 0x0a0a0c,
    surface: 0x131316,
    surface_2: 0x1c1c21,
    surface_3: 0x25252b,
    border: 0x2a2a30,
    border_strong: 0x8a8a96,
    text: 0xffffff,
    text_secondary: 0xececf0,
    text_muted: 0xa9a9b3,
    primary: 0xe60012,
    primary_text: 0xff4d5c,
    primary_soft: 0x3a0a10,
    on_primary: 0xffffff,
    ok: 0x7fd6a3,
    err: 0xff8a7a,
    info: 0x8fb7e8,
    muted: 0xa9a9b3,
    shadow_hard: 0x8c000000,
    shadow_dot: 0x59000000,
};

/// One entry of the selector, in Excubia's order. `colors` is None for Sistema, which reads Windows.
pub struct Preset {
    pub id: &'static str,
    name_es: &'static str,
    name_en: &'static str,
    note_es: &'static str,
    note_en: &'static str,
    pub colors: Option<Palette>,
}

impl Preset {
    pub fn name(&self) -> &'static str {
        tr(self.name_es, self.name_en)
    }

    pub fn note(&self) -> &'static str {
        tr(self.note_es, self.note_en)
    }
}

pub const PRESETS: &[Preset] = &[
    Preset {
        id: "system",
        name_es: "Sistema",
        name_en: "System",
        note_es: "Sigue el modo claro u oscuro de tu Windows",
        note_en: "Follows the light or dark mode of your Windows",
        colors: None,
    },
    Preset {
        id: "noche",
        name_es: "Noche",
        name_en: "Night",
        note_es: "Azul noche y latón",
        note_en: "Night blue and brass",
        colors: Some(NOCHE),
    },
    Preset {
        id: "pergamino",
        name_es: "Pergamino",
        name_en: "Parchment",
        note_es: "Papel cálido",
        note_en: "Warm paper",
        colors: Some(PERGAMINO),
    },
    Preset {
        id: "carbon",
        name_es: "Carbón",
        name_en: "Charcoal",
        note_es: "Gris oscuro",
        note_en: "Dark grey",
        colors: Some(CARBON),
    },
    Preset {
        id: "papel",
        name_es: "Papel",
        name_en: "Paper",
        note_es: "Blanco roto",
        note_en: "Off-white",
        colors: Some(PAPEL),
    },
    Preset {
        id: "medianoche",
        name_es: "Medianoche",
        name_en: "Midnight",
        note_es: "Azul profundo",
        note_en: "Deep blue",
        colors: Some(MEDIANOCHE),
    },
    Preset {
        id: "contraste",
        name_es: "Alto contraste",
        name_en: "High contrast",
        note_es: "Negro y blanco",
        note_en: "Black and white",
        colors: Some(CONTRASTE),
    },
    Preset {
        id: "telon",
        name_es: "Telón",
        name_en: "Curtain",
        note_es: "Rojo, negro y blanco de cómic",
        note_en: "Red, black and white comic",
        colors: Some(TELON),
    },
];

/// Windows' mode and accent, read for the Sistema palette.
#[derive(Clone, Copy)]
pub struct SystemLook {
    pub dark: bool,
    pub accent: u32,
}

/// The palette with this id. A missing or unknown id is Sistema.
pub fn preset(id: &str) -> &'static Preset {
    PRESETS.iter().find(|preset| preset.id == id).unwrap_or(&PRESETS[0])
}

/// The palette to paint with: a fixed one from the list, or Sistema built from Windows.
pub fn resolve(id: &str, look: SystemLook) -> Palette {
    preset(id).colors.unwrap_or_else(|| system(look))
}

/// Sistema: Carbón or Papel by Windows' mode, with the Windows accent as primary.
fn system(look: SystemLook) -> Palette {
    let base = if look.dark { CARBON } else { PAPEL };
    let accent = look.accent;
    Palette {
        primary: accent,
        primary_text: readable(accent, &[base.bg, base.surface], base.text),
        primary_soft: mix(base.surface, accent, 0.2),
        on_primary: if contrast(0xffffff, accent) >= contrast(0x000000, accent) { 0xffffff } else { 0x000000 },
        ..base
    }
}

/// The accent as text when it reads on every background; otherwise the first shade toward the text color that does.
fn readable(accent: u32, backgrounds: &[u32], toward: u32) -> u32 {
    (0..=20)
        .map(|step| mix(accent, toward, step as f32 / 20.0))
        .find(|candidate| backgrounds.iter().all(|bg| contrast(*candidate, *bg) >= 4.5))
        .unwrap_or(toward)
}

/// Windows stores the accent as ABGR: the lowest byte is red. Returns 0xRRGGBB.
pub fn accent_from_abgr(abgr: u32) -> u32 {
    let red = abgr & 0xff;
    let green = (abgr >> 8) & 0xff;
    let blue = (abgr >> 16) & 0xff;
    (red << 16) | (green << 8) | blue
}

/// Linear mix of two colors: `t` 0 is `a`, 1 is `b`.
fn mix(a: u32, b: u32, t: f32) -> u32 {
    let channel = |shift: u32| {
        let x = ((a >> shift) & 0xff) as f32;
        let y = ((b >> shift) & 0xff) as f32;
        (x + (y - x) * t).round() as u32
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

/// WCAG relative luminance of an sRGB color.
fn luminance(color: u32) -> f32 {
    let channel = |shift: u32| {
        let c = ((color >> shift) & 0xff) as f32 / 255.0;
        if c <= 0.03928 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0)
}

/// WCAG contrast ratio of two colors. Normal text needs 4.5 or more.
pub fn contrast(a: u32, b: u32) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// The letter color on an app's tile: white or the ink, whichever reads better on it.
pub fn ink_on(bg: u32) -> u32 {
    if contrast(0xffffff, bg) >= contrast(INK, bg) {
        0xffffff
    } else {
        INK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Windows looks that Sistema must handle: both modes, and accents that are dark, pale or bright.
    fn looks() -> Vec<SystemLook> {
        vec![
            SystemLook { dark: true, accent: 0x7fa6d6 },
            SystemLook { dark: false, accent: 0x2b6fb5 },
            SystemLook { dark: false, accent: 0xfff176 },
            SystemLook { dark: true, accent: 0x1f4e8c },
            SystemLook { dark: true, accent: 0xe81123 },
        ]
    }

    /// Every palette the selector can show, with Sistema in each of its looks.
    fn every_palette() -> Vec<(String, Palette)> {
        let mut all = Vec::new();
        for preset in PRESETS {
            match preset.colors {
                Some(colors) => all.push((preset.id.to_string(), colors)),
                None => {
                    for look in looks() {
                        let name = format!("system dark={} accent={:06x}", look.dark, look.accent);
                        all.push((name, resolve(preset.id, look)));
                    }
                }
            }
        }
        all
    }

    #[test]
    fn every_palette_meets_aa_on_its_text_pairs() {
        for (name, p) in every_palette() {
            let pairs = [
                (p.text, p.bg),
                (p.text, p.surface),
                (p.text_secondary, p.surface),
                (p.text_muted, p.surface),
                (p.on_primary, p.primary),
                (p.text, p.surface_2),
                (p.text_secondary, p.bg),
                (p.text_muted, p.bg),
            ];
            for (fg, bg) in pairs {
                assert!(contrast(fg, bg) >= 4.5, "{name}: #{fg:06x} on #{bg:06x} is {:.2}:1", contrast(fg, bg));
            }
        }
    }

    #[test]
    fn links_and_status_chips_meet_aa_too() {
        for (name, p) in every_palette() {
            for bg in [p.bg, p.surface] {
                assert!(contrast(p.primary_text, bg) >= 4.5, "{name}: link #{:06x} on #{bg:06x}", p.primary_text);
            }
            for color in [p.ok, p.err, p.info, p.muted] {
                let (fg, bg) = p.tone(color);
                assert!(contrast(fg, bg) >= 4.5, "{name}: chip #{fg:06x} on #{bg:06x}");
            }
        }
    }

    #[test]
    fn windows_accent_is_read_as_abgr() {
        // 0xFFD6A67F stores the blue #7FA6D6 with the red in the lowest byte.
        assert_eq!(accent_from_abgr(0xffd6a67f), 0x7fa6d6);
    }

    #[test]
    fn on_primary_is_black_or_white_by_contrast() {
        let on = |accent| resolve("system", SystemLook { dark: true, accent }).on_primary;
        assert_eq!(on(0x7fa6d6), 0x000000);
        assert_eq!(on(0x1f4e8c), 0xffffff);
    }

    #[test]
    fn sistema_follows_windows_mode_and_accent() {
        let dark = resolve("system", SystemLook { dark: true, accent: 0x7fa6d6 });
        assert_eq!(dark.bg, CARBON.bg);
        assert_eq!(dark.primary, 0x7fa6d6);
        let light = resolve("system", SystemLook { dark: false, accent: 0x2b6fb5 });
        assert_eq!(light.bg, PAPEL.bg);
        assert_eq!(light.primary_text, 0x2b6fb5);
    }

    #[test]
    fn a_pale_accent_is_darkened_until_it_reads_on_paper() {
        let p = resolve("system", SystemLook { dark: false, accent: 0xfff176 });
        assert_eq!(p.primary, 0xfff176);
        assert_ne!(p.primary_text, 0xfff176);
        assert!(contrast(p.primary_text, p.bg) >= 4.5 && contrast(p.primary_text, p.surface) >= 4.5);
    }

    #[test]
    fn missing_or_unknown_palette_is_sistema() {
        assert_eq!(preset("").id, "system");
        assert_eq!(preset("neon").id, "system");
        assert_eq!(preset("noche").id, "noche");
    }

    #[test]
    fn the_selector_keeps_excubias_order() {
        let ids: Vec<&str> = PRESETS.iter().map(|preset| preset.id).collect();
        assert_eq!(
            ids,
            ["system", "noche", "pergamino", "carbon", "papel", "medianoche", "contraste", "telon"]
        );
    }

    #[test]
    fn ink_on_takes_white_or_the_ink_by_contrast() {
        assert_eq!(ink_on(0x10b981), INK); // Excubia
        assert_eq!(ink_on(0x6366f1), 0xffffff); // Download Lovable
        assert_eq!(ink_on(0xf59e0b), INK); // ARK ASA Config Manager
    }
}
