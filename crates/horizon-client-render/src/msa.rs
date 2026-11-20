//! Microsoft sign-in inside the game, for the Switch, which has no launcher to sign in for it
//! (on a computer the launcher does it: `launcher/src/account.rs`, which this follows step for
//! step). The OAuth device-code flow (the player enters a code at microsoft.com/link on a phone),
//! then Xbox Live, XSTS and the Minecraft services, the ownership check and the profile. The
//! result goes to [`crate::auth::set_current`], so worlds and servers opened afterwards play as
//! the account.
//!
//! The account is kept in `account.json` in the game folder, with the Microsoft refresh token,
//! so the next launch signs in again without a code ([`resume`]). On the Switch nothing safer
//! exists: whoever has the SD card can use the file until the player signs out ([`sign_out`]
//! deletes it). Tokens never reach logs, errors or the screen.
//!
//! The Azure app (client ID) is configuration, as in the launcher: `HORIZON_MSA_CLIENT_ID` at run
//! time (env.txt on the Switch) or at build time.
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const SCOPE: &str = "XboxLive.signin offline_access";
const MICROSOFT: &str = "https://login.microsoftonline.com/consumers/oauth2/v2.0";
const XBOX_USER: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MINECRAFT: &str = "https://api.minecraftservices.com";
/// The Minecraft token is renewed when it has less than this left.
const RENEW_BEFORE: u64 = 3600;
/// The file next to the game's saves.
pub const FILE_NAME: &str = "account.json";

/// The configured Azure app: the run-time variable wins over the build-time one; empty means none.
pub fn client_id() -> Option<String> {
    std::env::var("HORIZON_MSA_CLIENT_ID")
        .ok()
        .or_else(|| option_env!("HORIZON_MSA_CLIENT_ID").map(str::to_owned))
        .filter(|id| !id.is_empty())
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn request(method: &str, url: &str) -> ureq::Request {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(20))
        .user_agent(&format!("Horizon/{}", horizon_core::release::VERSION))
        .build()
        .request(method, url)
}

enum Body<'a> {
    Form(Vec<(&'a str, &'a str)>),
    Json(Value),
}

/// One request: the status and the JSON body (null when there is none). Transport errors name
/// the service, never the URL or the request.
fn send(request: ureq::Request, body: Option<Body>, service: &str) -> Result<(u16, Value), String> {
    let result = match body {
        None => request.call(),
        Some(Body::Form(fields)) => request.send_form(&fields),
        Some(Body::Json(value)) => request
            .set("Content-Type", "application/json")
            .set("Accept", "application/json")
            .send_string(&value.to_string()),
    };
    let response = match result {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(ureq::Error::Transport(_)) => {
            return Err(format!(
                "Could not reach {service}. Check the Switch's internet connection."
            ));
        }
    };
    let status = response.status();
    let text = response.into_string().unwrap_or_default();
    Ok((status, serde_json::from_str(&text).unwrap_or(Value::Null)))
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}

// ----- The device-code flow ----------------------------------------------------------------------

/// What the player types at the verification page.
pub struct DeviceCode {
    pub user_code: String,
    pub verification_uri: String,
    interval: u64,
    expires_at: u64,
    device_code: String,
}

struct MsTokens {
    access: String,
    refresh: String,
}

fn device_code(client_id: &str) -> Result<DeviceCode, String> {
    let (status, v) = send(
        request("POST", &format!("{MICROSOFT}/devicecode")),
        Some(Body::Form(vec![("client_id", client_id), ("scope", SCOPE)])),
        "Microsoft",
    )?;
    match (
        status,
        str_field(&v, "user_code"),
        str_field(&v, "device_code"),
    ) {
        (200, Some(user_code), Some(device_code)) => Ok(DeviceCode {
            user_code,
            verification_uri: str_field(&v, "verification_uri")
                .unwrap_or_else(|| "https://www.microsoft.com/link".into()),
            interval: v
                .get("interval")
                .and_then(Value::as_u64)
                .unwrap_or(5)
                .max(1),
            expires_at: now() + v.get("expires_in").and_then(Value::as_u64).unwrap_or(900),
            device_code,
        }),
        _ => Err(oauth_error(&v)),
    }
}

fn oauth_error(v: &Value) -> String {
    match v.get("error").and_then(Value::as_str) {
        Some("authorization_declined") => "Sign-in was declined in the browser.".into(),
        Some("expired_token") => "The sign-in code expired. Try again.".into(),
        Some("bad_verification_code") => "The sign-in code was not recognised. Try again.".into(),
        Some("invalid_grant") => "Your Microsoft sign-in expired. Sign in again.".into(),
        Some("invalid_client" | "unauthorized_client") => {
            "Microsoft refused this build's app registration (client ID).".into()
        }
        Some(other) => format!("Microsoft sign-in failed ({other})."),
        None => "Microsoft sign-in failed.".into(),
    }
}

fn ms_tokens(status: u16, v: &Value) -> Result<MsTokens, String> {
    match (
        status,
        str_field(v, "access_token"),
        str_field(v, "refresh_token"),
    ) {
        (200, Some(access), Some(refresh)) => Ok(MsTokens { access, refresh }),
        _ => Err(oauth_error(v)),
    }
}

/// Poll the token endpoint as asked until the player has signed in, the code expires, or
/// `cancelled` says stop.
fn wait_for_sign_in(
    client_id: &str,
    code: &DeviceCode,
    cancelled: &dyn Fn() -> bool,
) -> Result<MsTokens, String> {
    let mut interval = code.interval;
    loop {
        let until = Instant::now() + Duration::from_secs(interval);
        while Instant::now() < until {
            if cancelled() {
                return Err(CANCELLED.into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        if now() > code.expires_at {
            return Err("The sign-in code expired. Try again.".into());
        }
        let (status, v) = send(
            request("POST", &format!("{MICROSOFT}/token")),
            Some(Body::Form(vec![
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ("client_id", client_id),
                ("device_code", &code.device_code),
            ])),
            "Microsoft",
        )?;
        match v.get("error").and_then(Value::as_str) {
            Some("authorization_pending") => {}
            Some("slow_down") => interval += 5,
            _ => return ms_tokens(status, &v),
        }
    }
}

const CANCELLED: &str = "Sign-in cancelled.";

fn refresh_ms(client_id: &str, refresh: &str) -> Result<MsTokens, String> {
    let (status, v) = send(
        request("POST", &format!("{MICROSOFT}/token")),
        Some(Body::Form(vec![
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh),
            ("scope", SCOPE),
        ])),
        "Microsoft",
    )?;
    ms_tokens(status, &v)
}

// ----- Xbox Live, XSTS, Minecraft ----------------------------------------------------------------

fn xsts_error(code: u64) -> String {
    match code {
        2148916233 => "This Microsoft account has no Xbox profile yet. Sign in once at xbox.com to create one, then try again.".into(),
        2148916235 => "Xbox Live is not available in your country or region.".into(),
        2148916236 | 2148916237 => "This account needs adult verification on xbox.com before it can play.".into(),
        2148916238 => "This is a child account: an adult has to add it to a Microsoft family first.".into(),
        other => format!("Xbox Live refused the sign-in (error {other})."),
    }
}

struct Minecraft {
    token: String,
    expires_at: u64,
    xuid: Option<String>,
}

fn minecraft_login(ms_access: &str) -> Result<Minecraft, String> {
    let (status, xbl) = send(
        request("POST", XBOX_USER),
        Some(Body::Json(json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_access}"),
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        }))),
        "Xbox Live",
    )?;
    let xbl_token = str_field(&xbl, "Token")
        .filter(|_| status == 200)
        .ok_or("Xbox Live refused the sign-in.")?;
    let (status, xsts) = send(
        request("POST", XSTS),
        Some(Body::Json(json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT",
        }))),
        "Xbox Live",
    )?;
    if let Some(code) = xsts.get("XErr").and_then(Value::as_u64) {
        return Err(xsts_error(code));
    }
    let claims = xsts.pointer("/DisplayClaims/xui/0");
    let (Some(xsts_token), Some(uhs)) = (
        str_field(&xsts, "Token").filter(|_| status == 200),
        claims.and_then(|c| str_field(c, "uhs")),
    ) else {
        return Err("Xbox Live refused the sign-in.".into());
    };
    let xuid = claims.and_then(|c| str_field(c, "xid"));
    let xtoken = format!("XBL3.0 x={uhs};{xsts_token}");
    let lifetime = |v: &Value| v.get("expires_in").and_then(Value::as_u64).unwrap_or(86400);
    // `launcher/login` first, as the launcher does (a new account's `login_with_xbox` token names
    // no profile); `login_with_xbox` stays the fallback.
    if let Ok((200, v)) = send(
        request("POST", &format!("{MINECRAFT}/launcher/login")),
        Some(Body::Json(
            json!({ "xtoken": xtoken, "platform": "PC_LAUNCHER" }),
        )),
        "the Minecraft services",
    ) && let Some(token) = str_field(&v, "access_token")
    {
        return Ok(Minecraft {
            token,
            expires_at: now() + lifetime(&v),
            xuid,
        });
    }
    let (status, mc) = send(
        request(
            "POST",
            &format!("{MINECRAFT}/authentication/login_with_xbox"),
        ),
        Some(Body::Json(json!({ "identityToken": xtoken }))),
        "the Minecraft services",
    )?;
    match (status, str_field(&mc, "access_token")) {
        (200, Some(token)) => Ok(Minecraft {
            token,
            expires_at: now() + lifetime(&mc),
            xuid,
        }),
        // New Azure apps are refused until Mojang approves them (Minecraft API access).
        (403, _) => Err(
            "The Minecraft services refused this build: its app is not approved by Mojang yet."
                .into(),
        ),
        (429, _) => Err("Too many sign-ins in a short time. Wait a minute and try again.".into()),
        _ => Err("The Minecraft services refused the sign-in.".into()),
    }
}

fn bearer(request: ureq::Request, token: &str) -> ureq::Request {
    request.set("Authorization", &format!("Bearer {token}"))
}

/// Whether the account owns Minecraft: Java Edition (Game Pass included).
fn owns_game(token: &str) -> Result<bool, String> {
    let (status, v) = send(
        bearer(
            request("GET", &format!("{MINECRAFT}/entitlements/mcstore")),
            token,
        ),
        None,
        "the Minecraft services",
    )?;
    if status != 200 {
        return Err("Could not check which games this account owns.".into());
    }
    Ok(v.get("items")
        .and_then(Value::as_array)
        .is_some_and(|items| {
            items.iter().any(|i| {
                matches!(
                    i.get("name").and_then(Value::as_str),
                    Some("game_minecraft" | "product_minecraft")
                )
            })
        }))
}

/// The profile's name and UUID (without dashes); none when the account has no profile.
fn profile(token: &str) -> Result<Option<(String, String)>, String> {
    let (status, v) = send(
        bearer(
            request("GET", &format!("{MINECRAFT}/minecraft/profile")),
            token,
        ),
        None,
        "the Minecraft services",
    )?;
    if status == 404 {
        return Ok(None);
    }
    match (str_field(&v, "name"), str_field(&v, "id")) {
        (Some(name), Some(id)) => Ok(Some((name, id.replace('-', "").to_lowercase()))),
        _ => Err("Could not read the Minecraft profile.".into()),
    }
}

// ----- The saved account -------------------------------------------------------------------------

/// What `account.json` keeps.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct Saved {
    name: String,
    /// The profile UUID without dashes.
    uuid: String,
    #[serde(default)]
    xuid: Option<String>,
    refresh_token: String,
    minecraft_token: String,
    expires_at: u64,
}

fn save(path: &Path, saved: &Saved) -> Result<(), String> {
    let text = serde_json::to_string_pretty(saved).map_err(|e| e.to_string())?;
    // Written whole beside it, then renamed: a crash never leaves half a file.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)
        .and_then(|()| std::fs::rename(&tmp, path))
        .map_err(|e| format!("Could not save the sign-in: {e}"))
}

fn session_of(saved: &Saved) -> Result<crate::auth::AuthSession, String> {
    let uuid = crate::auth::parse_uuid(&saved.uuid).ok_or("The saved account is damaged.")?;
    Ok(crate::auth::AuthSession::signed_in(
        saved.name.clone(),
        uuid,
        saved.xuid.clone(),
        saved.minecraft_token.clone(),
    ))
}

/// From fresh Microsoft tokens: the Minecraft token, ownership, profile; saved and returned.
/// The Xbox, XSTS and Minecraft steps after Microsoft's: the account to keep, not saved yet.
fn account(ms: MsTokens) -> Result<Saved, String> {
    let mc = minecraft_login(&ms.access)?;
    if !owns_game(&mc.token)? {
        return Err("This account doesn't own Minecraft: Java Edition.".into());
    }
    let (name, uuid) = profile(&mc.token)?.ok_or(
        "This account has no Minecraft profile yet. Create one at minecraft.net, then sign in again.",
    )?;
    let saved = Saved {
        name,
        uuid,
        xuid: mc.xuid,
        refresh_token: ms.refresh,
        minecraft_token: mc.token,
        expires_at: mc.expires_at,
    };
    Ok(saved)
}

/// The saved account's name, without signing in (for the title screen before [`resume`] ends).
pub fn saved_name(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join(FILE_NAME)).ok()?;
    serde_json::from_str::<Saved>(&text).ok().map(|s| s.name)
}

/// Sign the saved account in again: its Minecraft token while it has an hour left, else a new
/// one from the refresh token. Blocking (network): run it off the main thread.
pub fn resume(dir: &Path) -> Result<Option<String>, String> {
    let signed_out = *sign_outs();
    let path = dir.join(FILE_NAME);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    let saved: Saved = serde_json::from_str(&text).map_err(|_| "The saved account is damaged.")?;
    let renewed = saved.expires_at <= now() + RENEW_BEFORE;
    let saved = if renewed {
        let client_id = client_id().ok_or(NOT_CONFIGURED)?;
        account(refresh_ms(&client_id, &saved.refresh_token)?)?
    } else {
        saved
    };
    let session = session_of(&saved)?;
    // A sign-out while this ran (the player was quicker than the network) wins.
    let sign_outs = sign_outs();
    if *sign_outs != signed_out {
        return Ok(None);
    }
    if renewed {
        save(&path, &saved)?;
    }
    crate::auth::set_current(session);
    Ok(Some(saved.name))
}

/// How many times the player signed out; a renewal that started before one doesn't sign in.
static SIGN_OUTS: Mutex<u64> = Mutex::new(0);

fn sign_outs() -> std::sync::MutexGuard<'static, u64> {
    SIGN_OUTS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Forget the account: the file goes, and the game plays under the offline name again.
pub fn sign_out(dir: &Path) {
    let mut sign_outs = sign_outs();
    *sign_outs += 1;
    let _ = std::fs::remove_file(dir.join(FILE_NAME));
    crate::auth::clear_current();
}

pub const NOT_CONFIGURED: &str = "This build has no Microsoft sign-in (HORIZON_MSA_CLIENT_ID).";

// ----- A sign-in running in the background, for the title screen ---------------------------------

/// Where a [`SignIn`] is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// Asking Microsoft for a code, or renewing the saved account.
    Contacting,
    /// The player enters `code` at `url`.
    Code {
        url: String,
        code: String,
    },
    /// Signed in at Microsoft; the Xbox and Minecraft steps run.
    Finishing,
    SignedIn {
        name: String,
    },
    Failed(String),
}

/// A sign-in on its own thread; the title screen reads [`SignIn::state`] each frame.
///
/// Cancelling is final: the account is saved and signed in under the same lock that [`cancel`]
/// takes, so after a cancel the sign-in neither saves the account nor signs it in, and a cancel
/// that comes just after it did undoes both ([`SignIn::cancel`]).
///
/// [`cancel`]: SignIn::cancel
pub struct SignIn {
    state: Arc<Mutex<State>>,
    commit: Arc<Mutex<Commit>>,
    dir: PathBuf,
}

/// Whether the sign-in was cancelled, and whether it saved and signed in the account.
#[derive(Default)]
struct Commit {
    cancelled: bool,
    done: bool,
}

fn lock(commit: &Mutex<Commit>) -> std::sync::MutexGuard<'_, Commit> {
    commit.lock().unwrap_or_else(|e| e.into_inner())
}

impl SignIn {
    /// A new sign-in with a code, saving the account in `dir`.
    pub fn start(dir: PathBuf) -> Self {
        let state = Arc::new(Mutex::new(State::Contacting));
        let commit = Arc::new(Mutex::new(Commit::default()));
        let (shared, stop) = (state.clone(), commit.clone());
        let home = dir.clone();
        let set = move |s: State| *shared.lock().unwrap_or_else(|e| e.into_inner()) = s;
        let spawned = std::thread::Builder::new()
            .name("Microsoft sign-in".into())
            .spawn(move || {
                let run = || -> Result<String, String> {
                    let client_id = client_id().ok_or(NOT_CONFIGURED)?;
                    let code = device_code(&client_id)?;
                    set(State::Code {
                        url: code.verification_uri.clone(),
                        code: code.user_code.clone(),
                    });
                    let cancelled = || lock(&stop).cancelled;
                    let ms = wait_for_sign_in(&client_id, &code, &cancelled)?;
                    set(State::Finishing);
                    let saved = account(ms)?;
                    let session = session_of(&saved)?;
                    // Saved and signed in only if not cancelled, and never after a cancel.
                    let mut commit = lock(&stop);
                    if commit.cancelled {
                        return Err(CANCELLED.into());
                    }
                    save(&home.join(FILE_NAME), &saved)?;
                    crate::auth::set_current(session);
                    commit.done = true;
                    // The title shows it signed in under the same lock: a Cancel pressed until
                    // then still finds it busy, and undoes it.
                    set(State::SignedIn {
                        name: saved.name.clone(),
                    });
                    drop(commit);
                    Ok(saved.name)
                };
                match run() {
                    Ok(name) => log::info!("Horizon: signed in to Microsoft as {name}"),
                    Err(e) => {
                        log::warn!("Horizon: Microsoft sign-in: {e}");
                        set(State::Failed(e));
                    }
                }
            });
        if spawned.is_err() {
            *state.lock().unwrap_or_else(|e| e.into_inner()) =
                State::Failed("Could not start the sign-in.".into());
        }
        Self { state, commit, dir }
    }

    pub fn state(&self) -> State {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// The player cancelled: the sign-in stops (waiting for the code, within a tenth of a
    /// second) and never saves or signs in the account. If it did just before (the screen still
    /// showed it busy), that is undone: the account file goes and the game is signed out.
    pub fn cancel(&self) {
        let mut commit = lock(&self.commit);
        if commit.cancelled {
            return;
        }
        commit.cancelled = true;
        if commit.done {
            sign_out(&self.dir);
            *self.state.lock().unwrap_or_else(|e| e.into_inner()) = State::Failed(CANCELLED.into());
            log::info!("Horizon: Microsoft sign-in cancelled: signed out again");
        }
    }
}

impl Drop for SignIn {
    /// Leaving the screen stops a sign-in still under way; one that finished stays signed in.
    fn drop(&mut self) {
        lock(&self.commit).cancelled = true;
    }
}

/// Renew the saved account in `dir` on a thread of its own at startup, so the title screen
/// isn't held up by the network. Its outcome is logged; the title screen shows the account
/// once [`crate::auth::current`] has it.
pub fn resume_in_background(dir: PathBuf) {
    if !dir.join(FILE_NAME).is_file() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("Microsoft sign-in".into())
        .spawn(move || match resume(&dir) {
            Ok(Some(name)) => {
                log::info!("Horizon: signed in to Microsoft as {name} (saved account)")
            }
            Ok(None) => {}
            Err(e) => log::warn!("Horizon: the saved Microsoft account could not sign in: {e}"),
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("hz-msa-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn saved(expires_at: u64) -> Saved {
        Saved {
            name: "Steve".into(),
            uuid: "069a79f444e94726a5befca90e38aaf5".into(),
            xuid: Some("2535".into()),
            refresh_token: "refresh".into(),
            minecraft_token: "mc-token".into(),
            expires_at,
        }
    }

    /// A saved token with more than an hour left signs in without the network, and the session
    /// carries the profile; signing out forgets both.
    #[test]
    fn a_fresh_saved_account_resumes_offline_and_signs_out() {
        let d = dir("fresh");
        save(&d.join(FILE_NAME), &saved(now() + 2 * RENEW_BEFORE)).unwrap();
        assert_eq!(saved_name(&d).as_deref(), Some("Steve"));
        assert_eq!(resume(&d).unwrap().as_deref(), Some("Steve"));
        let session = crate::auth::current().unwrap();
        assert_eq!(session.name, "Steve");
        assert_eq!(session.uuid, 0x069a79f444e94726a5befca90e38aaf5);
        sign_out(&d);
        assert!(crate::auth::current().is_none());
        assert!(!d.join(FILE_NAME).exists());
        assert_eq!(resume(&d).unwrap(), None);

        // A Cancel that comes just after a sign-in saved and signed in the account (the screen
        // still showed it busy) undoes both; leaving the screen after it finished doesn't.
        let finished = |d: &Path| {
            save(&d.join(FILE_NAME), &saved(now() + 2 * RENEW_BEFORE)).unwrap();
            crate::auth::set_current(session_of(&saved(now() + 2 * RENEW_BEFORE)).unwrap());
            SignIn {
                state: Arc::new(Mutex::new(State::SignedIn {
                    name: "Steve".into(),
                })),
                commit: Arc::new(Mutex::new(Commit {
                    cancelled: false,
                    done: true,
                })),
                dir: d.to_path_buf(),
            }
        };
        let sign_in = finished(&d);
        sign_in.cancel();
        assert!(crate::auth::current().is_none());
        assert!(!d.join(FILE_NAME).exists());
        assert!(matches!(sign_in.state(), State::Failed(_)));
        drop(finished(&d));
        assert!(crate::auth::current().is_some());
        assert!(d.join(FILE_NAME).exists());
        // A cancelled sign-in can't commit any more.
        let sign_in = SignIn {
            state: Arc::new(Mutex::new(State::Finishing)),
            commit: Arc::new(Mutex::new(Commit::default())),
            dir: d.clone(),
        };
        let commit = sign_in.commit.clone();
        drop(sign_in);
        assert!(lock(&commit).cancelled);
        sign_out(&d);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_damaged_file_is_an_error_not_a_session() {
        let d = dir("damaged");
        std::fs::write(d.join(FILE_NAME), "{not json").unwrap();
        assert!(resume(&d).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
