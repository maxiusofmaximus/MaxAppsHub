//! The desktop apps this Hub installs on Windows. The phone Hub keeps its own list.

/// How a release reaches the PC.
pub enum Delivery {
    /// The release is a Windows installer, run silently (`/S`). The app is found
    /// through its entry in the Windows uninstall list.
    Installer { display_name: &'static str },
    /// The release is one program file, kept in the Hub's own folder.
    Portable { folder: &'static str },
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
        delivery: Delivery::Installer { display_name: "Excubia" },
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
        delivery: Delivery::Installer { display_name: "ARK ASA Config Manager" },
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
};

pub fn find(name: &str) -> Option<&'static DesktopApp> {
    APPS.iter().find(|app| app.name == name)
}
