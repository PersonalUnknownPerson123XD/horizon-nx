//! The offline player name, when neither `--name`, `HORIZON_PLAYER_NAME` nor a
//! signed-in account gives one. Vanilla's client defaults `--username` to
//! `"Player" + System.currentTimeMillis() % 1000` (net.minecraft.client.main.Main)
//! and the official launcher always passes the account's name, so two players
//! rarely share one. Horizon used plain `Player` for everyone, which is also
//! the LAN host's own profile: every guest got the host's offline UUID and was
//! refused with "You logged in from another location". The generated name is
//! kept in `horizon-player.txt` next to `options.txt` so the same player gets
//! the same offline UUID (and inventory on a friend's world) every time.
//!
//! The title screen's Account page lets the player pick this nickname. It is
//! the main play identity for singleplayer and offline-mode servers. The UUID
//! is vanilla's offline UUID (`MD5("OfflinePlayer:" + name)` with version-3 bits).
use std::{
    fs,
    path::Path,
    sync::Mutex,
};

pub const FILE: &str = "horizon-player.txt";
/// Saved beside [`FILE`]: the same name and its offline UUID, for the profile.
pub const PROFILE_FILE: &str = "offline-profile.json";
const KEY: &str = "name:";

static CURRENT: Mutex<Option<String>> = Mutex::new(None);

/// StringUtil.isValidPlayerName, plus not empty (the login screen sends at least one character).
pub fn is_valid(name: &str) -> bool {
    crate::lan::conn::valid_name(name)
}

/// `Player` and five digits (vanilla's default has three: one shared name in a
/// thousand pairs is too many for a game that keeps it).
pub fn generate(seed: u64) -> String {
    format!("Player{:05}", seed % 100_000)
}

/// The name chosen for this process: a saved or typed nickname, `--name`, or
/// `"Player"` before anything has been set.
pub fn current() -> String {
    CURRENT
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .filter(|n| is_valid(n))
        .unwrap_or_else(|| "Player".to_owned())
}

/// Remember `name` for this process (singleplayer and offline-mode servers).
pub fn set_current(name: impl Into<String>) {
    let name = name.into();
    *CURRENT.lock().unwrap_or_else(|e| e.into_inner()) = Some(name);
}

/// Vanilla's offline UUID for [`current`].
pub fn uuid() -> u128 {
    horizon_protocol::crypto::offline_uuid(&current())
}

/// The UUID as the four signed ints the level data stores.
pub fn uuid_ints() -> [i32; 4] {
    uuid_to_ints(uuid())
}

/// Split a 128-bit UUID into the four ints of a player profile.
pub fn uuid_to_ints(u: u128) -> [i32; 4] {
    [
        (u >> 96) as u32 as i32,
        (u >> 64) as u32 as i32,
        (u >> 32) as u32 as i32,
        u as u32 as i32,
    ]
}

/// Hyphenated form of the offline UUID for `name`.
pub fn uuid_hyphenated(name: &str) -> String {
    let s = format!("{:032x}", horizon_protocol::crypto::offline_uuid(name));
    format!(
        "{}-{}-{}-{}-{}",
        &s[0..8],
        &s[8..12],
        &s[12..16],
        &s[16..20],
        &s[20..32]
    )
}

/// The name already kept in `dir`, if the file is there and the name is valid.
pub fn load(dir: &Path) -> Option<String> {
    let path = dir.join(FILE);
    if let Some(name) = fs::read_to_string(&path).ok().and_then(|text| parse(&text)) {
        return Some(name);
    }
    parse_profile(&fs::read_to_string(dir.join(PROFILE_FILE)).ok()?)
}

/// Write the nickname (and its offline UUID) in `dir`, and make it current.
pub fn save(dir: &Path, name: &str) -> std::io::Result<()> {
    if !is_valid(name) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid player name",
        ));
    }
    fs::create_dir_all(dir)?;
    fs::write(dir.join(FILE), format!("{KEY}{name}\n"))?;
    let profile = format!(
        "{{\n  \"kind\": \"offline\",\n  \"name\": {},\n  \"uuid\": {}\n}}\n",
        serde_json::to_string(name).unwrap_or_else(|_| format!("\"{name}\"")),
        serde_json::to_string(&uuid_hyphenated(name)).unwrap_or_default(),
    );
    if let Err(e) = fs::write(dir.join(PROFILE_FILE), profile) {
        log::warn!(
            "Horizon: could not keep the offline profile in {}: {e}",
            dir.join(PROFILE_FILE).display()
        );
    }
    set_current(name.to_owned());
    Ok(())
}

/// The name kept in `dir`, created on first use. A file that can't be written
/// still yields a name for this run.
pub fn load_or_create(dir: &Path) -> String {
    if let Some(name) = load(dir) {
        set_current(name.clone());
        return name;
    }
    let name = generate(seed());
    if let Err(e) = save(dir, &name) {
        log::warn!(
            "Horizon: could not keep the player name in {}: {e}",
            dir.join(FILE).display()
        );
        set_current(name.clone());
    }
    name
}

fn parse(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.trim_end_matches('\r').strip_prefix(KEY))
        .filter(|name| is_valid(name))
        .map(str::to_owned)
}

fn parse_profile(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let name = value.get("name")?.as_str()?;
    is_valid(name).then(|| name.to_owned())
}

fn seed() -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    // Two players starting the game in the same millisecond still differ.
    let mut x = nanos ^ u64::from(std::process::id()).rotate_left(32);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^ (x >> 33)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// `CURRENT` is process-wide; tests that read or write it take this lock.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "horizon-offline-name-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn generated_names_are_valid_and_never_the_hosts_profile() {
        for seed in [0, 7, 99_999, 100_000, u64::MAX] {
            let name = generate(seed);
            assert!(is_valid(&name), "{name}");
            // The LAN host's own profile (game/chat.rs PLAYER_NAME).
            assert_ne!(name, "Player");
            assert_eq!(name.len(), 11);
        }
        assert_eq!(generate(42), "Player00042");
    }

    #[test]
    fn the_name_is_kept_and_reused() {
        let _guard = TEST_LOCK.lock().unwrap();
        *CURRENT.lock().unwrap() = None;
        let d = dir("kept");
        let first = load_or_create(&d);
        assert_eq!(
            fs::read_to_string(d.join(FILE)).unwrap(),
            format!("name:{first}\n")
        );
        assert_eq!(load_or_create(&d), first);
        // An edited name is used as it is.
        fs::write(d.join(FILE), "name:Alex_2\r\n").unwrap();
        assert_eq!(load_or_create(&d), "Alex_2");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn invalid_names_are_replaced() {
        let _guard = TEST_LOCK.lock().unwrap();
        *CURRENT.lock().unwrap() = None;
        for text in [
            "",
            "name:",
            "name:has space",
            "name:ThisNameIsTooLong17",
            "name:été",
        ] {
            let d = dir("invalid");
            fs::write(d.join(FILE), text).unwrap();
            let name = load_or_create(&d);
            assert!(
                name.starts_with("Player") && is_valid(&name),
                "{text:?} -> {name}"
            );
            assert_eq!(load_or_create(&d), name);
            let _ = fs::remove_dir_all(&d);
        }
    }

    #[test]
    fn save_writes_the_profile_and_sets_the_current_name() {
        let _guard = TEST_LOCK.lock().unwrap();
        *CURRENT.lock().unwrap() = None;
        let d = dir("save");
        save(&d, "SwitchKid").unwrap();
        assert_eq!(current(), "SwitchKid");
        assert_eq!(load(&d).as_deref(), Some("SwitchKid"));
        let profile = fs::read_to_string(d.join(PROFILE_FILE)).unwrap();
        assert!(profile.contains("\"kind\": \"offline\""), "{profile}");
        assert!(profile.contains("SwitchKid"), "{profile}");
        assert!(profile.contains(&uuid_hyphenated("SwitchKid")), "{profile}");
        assert_eq!(uuid(), horizon_protocol::crypto::offline_uuid("SwitchKid"));
        let _ = fs::remove_dir_all(&d);
        // Leave other tests a clean default.
        *CURRENT.lock().unwrap() = None;
    }

    #[test]
    fn a_profile_json_is_enough_when_the_txt_is_missing() {
        let _guard = TEST_LOCK.lock().unwrap();
        *CURRENT.lock().unwrap() = None;
        let d = dir("profile-only");
        fs::write(
            d.join(PROFILE_FILE),
            r#"{"kind":"offline","name":"OnlyJson","uuid":"00000000-0000-0000-0000-000000000000"}"#,
        )
        .unwrap();
        assert_eq!(load(&d).as_deref(), Some("OnlyJson"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn current_defaults_to_player_until_set() {
        let _guard = TEST_LOCK.lock().unwrap();
        *CURRENT.lock().unwrap() = None;
        *CURRENT.lock().unwrap() = None;
        assert_eq!(current(), "Player");
        set_current("Alex");
        assert_eq!(current(), "Alex");
        *CURRENT.lock().unwrap() = None;
    }
}
