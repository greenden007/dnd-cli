use anyhow::Result;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub type CacheManifest = HashMap<String, String>;

pub fn manifest_path(data_root: &Path) -> PathBuf {
    data_root.join(".cache_manifest.json")
}

pub fn load_manifest(data_root: &Path) -> Result<CacheManifest> {
    let path = manifest_path(data_root);
    if !path.exists() {
        return Ok(HashMap::new());
    }
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

pub fn save_manifest(data_root: &Path, manifest: &CacheManifest) -> Result<()> {
    fs::create_dir_all(data_root)?;
    let path = manifest_path(data_root);
    let temp_path = data_root.join(".cache_manifest.json.tmp");
    let contents = serde_json::to_vec_pretty(manifest)?;
    let mut file = fs::File::create(&temp_path)?;
    file.write_all(&contents)?;
    file.sync_all()?;
    fs::rename(temp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{load_manifest, manifest_path, save_manifest, CacheManifest};
    use std::collections::HashMap;
    use tempfile::tempdir;

    #[test]
    fn missing_manifest_is_an_empty_cache() {
        let directory = tempdir().unwrap();
        assert!(load_manifest(directory.path()).unwrap().is_empty());
    }

    #[test]
    fn manifest_round_trips_atomically() {
        let directory = tempdir().unwrap();
        let mut manifest: CacheManifest = HashMap::new();
        manifest.insert("character/one".to_string(), "session-a".to_string());

        save_manifest(directory.path(), &manifest).unwrap();

        assert_eq!(load_manifest(directory.path()).unwrap(), manifest);
        assert!(manifest_path(directory.path()).exists());
        assert!(!directory.path().join(".cache_manifest.json.tmp").exists());
    }

    #[test]
    fn malformed_manifest_is_not_silently_accepted() {
        let directory = tempdir().unwrap();
        std::fs::write(manifest_path(directory.path()), "not-json").unwrap();
        assert!(load_manifest(directory.path()).is_err());
    }
}
