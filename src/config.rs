use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use url::Url;

pub const DEFAULT_SERVER: &str = "https://archerdnd.tech/api";
const SERVER_CONFIG_FILE: &str = "server.json";
const PROFILE_STORE_FILE: &str = "profiles.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerProfile {
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileStore {
    pub active: String,
    pub profiles: BTreeMap<String, ServerProfile>,
}

impl ServerProfile {
    pub fn new(base_url: impl AsRef<str>) -> Result<Self> {
        let parsed = Url::parse(base_url.as_ref().trim())
            .with_context(|| format!("Invalid server URL: {}", base_url.as_ref()))?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host().is_none() {
            return Err(anyhow!("Server URL must include an http or https scheme and host"));
        }
        let mut normalized = parsed.to_string();
        while normalized.ends_with('/') {
            normalized.pop();
        }
        Ok(Self { base_url: normalized })
    }

    pub fn url(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            return path.to_string();
        }
        format!("{}{}", self.base_url, if path.starts_with('/') { path.to_string() } else { format!("/{path}") })
    }
}

fn config_dir() -> Result<PathBuf> {
    Ok(dirs::home_dir()
        .ok_or_else(|| anyhow!("Could not find home directory"))?
        .join(".archerdndsys"))
}

pub fn server_config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join(SERVER_CONFIG_FILE))
}

pub fn profile_store_path() -> Result<PathBuf> {
    Ok(config_dir()?.join(PROFILE_STORE_FILE))
}

fn profile_id(profile: &ServerProfile) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in profile.base_url.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

pub fn data_dir() -> Result<PathBuf> {
    let profile = load_server_profile()?;
    let scoped = config_dir()?.join("servers").join(profile_id(&profile));
    let legacy = config_dir()?;
    if scoped.exists() || profile.base_url != DEFAULT_SERVER || !legacy.join(".auth_tokens.txt").exists() {
        Ok(scoped)
    } else {
        Ok(legacy)
    }
}

pub fn load_profile_store() -> Result<ProfileStore> {
    let path = profile_store_path()?;
    if !path.exists() {
        let profile = load_legacy_profile()?;
        let mut profiles = BTreeMap::new();
        profiles.insert("default".to_string(), profile);
        return Ok(ProfileStore { active: "default".to_string(), profiles });
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn load_legacy_profile() -> Result<ServerProfile> {
    let path = server_config_path()?;
    if !path.exists() { return ServerProfile::new(DEFAULT_SERVER); }
    let profile: ServerProfile = serde_json::from_str(&fs::read_to_string(path)?)?;
    ServerProfile::new(profile.base_url)
}

pub fn load_server_profile() -> Result<ServerProfile> {
    let store_path = profile_store_path()?;
    if store_path.exists() {
        let store: ProfileStore = serde_json::from_str(&fs::read_to_string(store_path)?)?;
        let profile = store.profiles.get(&store.active)
            .ok_or_else(|| anyhow!("Active server profile '{}' was not found", store.active))?;
        return ServerProfile::new(&profile.base_url);
    }
    load_legacy_profile()
}

pub fn save_server_profile(profile: &ServerProfile) -> Result<()> {
    let directory = config_dir()?;
    fs::create_dir_all(&directory)?;
    let path = server_config_path()?;
    let contents = serde_json::to_string_pretty(profile)?;
    fs::write(&path, format!("{contents}\n"))
        .with_context(|| format!("Failed to save server profile: {}", path.display()))?;
    Ok(())
}

pub fn save_named_profile(name: &str, profile: &ServerProfile, make_active: bool) -> Result<()> {
    if name.trim().is_empty() { return Err(anyhow!("Profile name cannot be empty")); }
    let directory = config_dir()?;
    fs::create_dir_all(&directory)?;
    let mut store = load_profile_store()?;
    store.profiles.insert(name.trim().to_string(), profile.clone());
    if make_active { store.active = name.trim().to_string(); }
    fs::write(profile_store_path()?, format!("{}\n", serde_json::to_string_pretty(&store)?))?;
    fs::create_dir_all(data_dir()?)?;
    Ok(())
}

pub fn set_active_profile(name: &str) -> Result<()> {
    let mut store = load_profile_store()?;
    if !store.profiles.contains_key(name) { return Err(anyhow!("Unknown server profile: {name}")); }
    store.active = name.to_string();
    fs::write(profile_store_path()?, format!("{}\n", serde_json::to_string_pretty(&store)?))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ServerProfile, DEFAULT_SERVER};

    #[test]
    fn normalizes_server_url_without_trailing_slashes() {
        let profile = ServerProfile::new("https://example.test/api///").unwrap();
        assert_eq!(profile.base_url, "https://example.test/api");
        assert_eq!(profile.url("/auth/login"), "https://example.test/api/auth/login");
    }

    #[test]
    fn accepts_legacy_default_server() {
        let profile = ServerProfile::new(DEFAULT_SERVER).unwrap();
        assert_eq!(profile.base_url, DEFAULT_SERVER);
    }

    #[test]
    fn rejects_urls_without_http_scheme_or_host() {
        assert!(ServerProfile::new("localhost:5000/api").is_err());
        assert!(ServerProfile::new("ftp://example.test/api").is_err());
    }
}
