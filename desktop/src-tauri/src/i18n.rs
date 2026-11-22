//! The Hub's language. Spanish is the source text; English is chosen in Settings.

use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

pub fn set_english(on: bool) {
    ENGLISH.store(on, Ordering::SeqCst);
}

pub fn english() -> bool {
    ENGLISH.load(Ordering::SeqCst)
}

/// The text in the language the user chose.
pub fn tr(es: &'static str, en: &'static str) -> &'static str {
    if english() {
        en
    } else {
        es
    }
}
