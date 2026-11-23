//! The desktop apps this Hub installs on Windows. The phone Hub keeps its own list.

/// How a release reaches the PC.
pub enum Delivery {
    /// The release is a Windows installer, run silently with the flags of its `kind`. The app is found
    /// through its entry in the Windows uninstall list.
    Installer { display_name: &'static str, kind: InstallerKind },
    /// The release is one program file, kept in the Hub's own folder.
    Portable { folder: &'static str },
}

/// Which installer framework built the release: each one has its own silent flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallerKind {
    /// NSIS: `/S`, and `/D=` for the folder.
    Nsis,
    /// Inno Setup: `/VERYSILENT`, and `/DIR=` for the folder.
    // Not built by any listed app yet; it is for the ConsoleEngine installer.
    #[allow(dead_code)]
    Inno,
}

/// One desktop app the Hub can install, update and open.
pub struct DesktopApp {
    pub name: &'static str,
    pub description: &'static str,
    pub description_en: &'static str,
    /// Accent color, the same family as the phone Hub.
    pub accent: u32,
    pub owner: &'static str,
    pub repo: &'static str,
    /// Releases whose tag starts with this prefix are this app's releases.
    pub tag_prefix: &'static str,
    /// The release asset whose name ends with this is the program.
    pub asset_suffix: &'static str,
    pub delivery: Delivery,
    /// The languages the app speaks, by name. Empty when not known: nothing is shown.
    pub languages: &'static [&'static str],
    /// Short Windows requirements, e.g. "Windows 10 u 11, 64 bits". Empty when not known: nothing is shown.
    pub requirements: &'static str,
    /// Whether the app's page shows "Qué hay de nuevo" from its release notes.
    pub show_notes: bool,
}

impl DesktopApp {
    /// The one-line description in the language the user chose.
    pub fn description(&self) -> &'static str {
        crate::i18n::tr(self.description, self.description_en)
    }
}

pub const APPS: &[DesktopApp] = &[
    DesktopApp {
        name: "Excubia",
        description: "Supervisión de agentes de IA desde el PC y el móvil",
        description_en: "Supervise AI agents from your PC and phone",
        accent: 0x10b981,
        owner: "maxiusofmaximus",
        repo: "excubia",
        tag_prefix: "desktop-v",
        asset_suffix: "-setup.exe",
        delivery: Delivery::Installer { display_name: "Excubia", kind: InstallerKind::Nsis },
        // Spanish only for now; English is a pending translation in the Excubia repo.
        languages: &["Español"],
        // From the README of maxiusofmaximus/excubia: "Escritorio (Windows 10/11, 64 bits)".
        requirements: "Windows 10 u 11, 64 bits",
        show_notes: true,
    },
    DesktopApp {
        name: "Download Lovable",
        description: "Descarga páginas de Lovable.dev para verlas sin conexión",
        description_en: "Download Lovable.dev pages to view them offline",
        accent: 0x6366f1,
        owner: "maxiusofmaximus",
        repo: "download-lovable",
        tag_prefix: "v",
        asset_suffix: ".exe",
        delivery: Delivery::Portable { folder: "download-lovable" },
        languages: &[],
        requirements: "",
        // The notes are the publisher's, not news for the user: hidden.
        show_notes: false,
    },
    DesktopApp {
        name: "ARK ASA Config Manager",
        description: "Configuración del servidor dedicado de ARK: Survival Ascended",
        description_en: "Settings for the ARK: Survival Ascended dedicated server",
        accent: 0xf59e0b,
        owner: "maxiusofmaximus",
        repo: "ArkASA-Servidor-Dedicado",
        tag_prefix: "v",
        asset_suffix: "-setup.exe",
        // Not verified on this PC yet: the name the installer writes is assumed.
        delivery: Delivery::Installer { display_name: "ARK ASA Config Manager", kind: InstallerKind::Nsis },
        // From frontend/src/i18n/translations.ts of its repo: en, es, de, pt, fr.
        languages: &["English", "Español", "Deutsch", "Português", "Français"],
        requirements: "",
        show_notes: true,
    },
    DesktopApp {
        name: "NMC Total",
        description: "Monitoreo de red en tiempo real con búsqueda en VirusTotal",
        description_en: "Real-time network monitoring with VirusTotal lookup",
        accent: 0xef4444,
        owner: "maxiusofmaximus",
        repo: "NMC-Total",
        tag_prefix: "v",
        asset_suffix: "-setup.exe",
        // Not verified on this PC yet: the name the installer writes is assumed.
        delivery: Delivery::Installer { display_name: "NMC Total", kind: InstallerKind::Nsis },
        languages: &[],
        requirements: "",
        // Its release notes are developer notes (versions, registry, test text): hidden.
        show_notes: false,
    },
];

/// The Hub itself. It updates from its own repository and is not listed as an app.
pub const SELF_APP: DesktopApp = DesktopApp {
    name: "MaxApps Hub",
    description: "",
    description_en: "",
    accent: 0x6366f1,
    owner: "maxiusofmaximus",
    repo: "MaxAppsHub",
    tag_prefix: "desktop-v",
    asset_suffix: "-desktop.exe",
    delivery: Delivery::Portable { folder: "maxapps-hub" },
    languages: &[],
    requirements: "",
    show_notes: false,
};

pub fn find(name: &str) -> Option<&'static DesktopApp> {
    APPS.iter().find(|app| app.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_app_names_its_repo_and_asset_and_nmc_total_is_listed() {
        for app in APPS {
            assert!(!app.repo.is_empty(), "{} has no repo", app.name);
            assert!(!app.asset_suffix.is_empty(), "{} has no asset suffix", app.name);
        }
        let nmc = find("NMC Total").expect("NMC Total is listed");
        assert_eq!(nmc.owner, "maxiusofmaximus");
    }

    #[test]
    fn download_lovable_hides_its_notes() {
        assert!(!find("Download Lovable").expect("Download Lovable is listed").show_notes);
    }
}
