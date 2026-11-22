//! The Switch build's stand-in for arboard (packaging/switch/cargo-patch.toml): Horizon OS has no
//! clipboard an NRO can reach, and arboard's Unix backends are X11 and Wayland. `Clipboard::new`
//! fails, so the game keeps its own copy of what it copied (`SystemClipboard`).

/// arboard's errors, as far as the game matches on them.
#[derive(Debug)]
pub enum Error {
    ContentNotAvailable,
    ClipboardNotSupported,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::ContentNotAvailable => "the clipboard is empty",
            Self::ClipboardNotSupported => "no system clipboard on this platform",
        })
    }
}

impl std::error::Error for Error {}

/// Never constructed: `new` reports that there is no clipboard.
pub struct Clipboard(());

impl Clipboard {
    pub fn new() -> Result<Self, Error> {
        Err(Error::ClipboardNotSupported)
    }
    pub fn get_text(&mut self) -> Result<String, Error> {
        Err(Error::ContentNotAvailable)
    }
    pub fn set_text(&mut self, _text: String) -> Result<(), Error> {
        Err(Error::ClipboardNotSupported)
    }
}
