//! API keys for translation services (DeepL, Gemini, Groq...), stored in keys.json
//! encrypted with Windows DPAPI: only this Windows account on this computer can read
//! them. The window can save or remove a key, and see whether one is saved, but never
//! reads a key back.

use std::{collections::HashMap, path::{Path, PathBuf}};

/// Services that can have a key
pub const SERVICES: &[&str] = &["deepl", "gemini", "groq", "openrouter", "openai", "custom"];

pub struct KeyStore {
    path: PathBuf,
}

impl KeyStore {
    pub fn new(data_dir: &Path) -> Self {
        Self { path: data_dir.join("keys.json") }
    }

    fn load(&self) -> HashMap<String, String> {
        std::fs::read(&self.path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn get(&self, service: &str) -> Option<String> {
        let hex = self.load().remove(service)?;
        let bytes = from_hex(&hex)?;
        String::from_utf8(unprotect(&bytes)?).ok()
    }

    pub fn set(&self, service: &str, key: &str) -> anyhow::Result<()> {
        anyhow::ensure!(SERVICES.contains(&service), "unknown service");
        let key = key.trim();
        anyhow::ensure!(key.len() <= 512 && !key.contains(char::is_whitespace), "that doesn't look like an API key");
        let mut all = self.load();
        if key.is_empty() {
            all.remove(service);
        } else {
            all.insert(service.to_owned(), to_hex(&protect(key.as_bytes())?));
        }
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(&all)?)?;
        std::fs::rename(tmp, &self.path)?;
        Ok(())
    }

    /// Which services have a key saved
    pub fn saved(&self) -> Vec<String> {
        let all = self.load();
        SERVICES.iter().filter(|s| all.contains_key(**s)).map(|s| s.to_string()).collect()
    }
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    (0..s.len())
        .step_by(2)
        .map(|i| s.get(i..i + 2).and_then(|h| u8::from_str_radix(h, 16).ok()))
        .collect()
}

#[cfg(windows)]
mod dpapi {
    use std::ffi::c_void;

    #[repr(C)]
    pub struct Blob {
        pub len: u32,
        pub data: *mut u8,
    }

    #[link(name = "crypt32")]
    extern "system" {
        pub fn CryptProtectData(
            input: *const Blob,
            description: *const u16,
            entropy: *const Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
        pub fn CryptUnprotectData(
            input: *const Blob,
            description: *mut *mut u16,
            entropy: *const Blob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        pub fn LocalFree(mem: *mut c_void) -> *mut c_void;
    }

    pub const UI_FORBIDDEN: u32 = 0x1;
}

#[cfg(windows)]
fn protect(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    use dpapi::*;
    let input = Blob { len: data.len() as u32, data: data.as_ptr() as *mut u8 };
    let mut output = Blob { len: 0, data: std::ptr::null_mut() };
    let ok = unsafe {
        CryptProtectData(&input, std::ptr::null(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), UI_FORBIDDEN, &mut output)
    };
    anyhow::ensure!(ok != 0, "couldn't encrypt the key");
    let out = unsafe { std::slice::from_raw_parts(output.data, output.len as usize).to_vec() };
    unsafe { LocalFree(output.data as *mut _) };
    Ok(out)
}

#[cfg(windows)]
fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    use dpapi::*;
    let input = Blob { len: data.len() as u32, data: data.as_ptr() as *mut u8 };
    let mut output = Blob { len: 0, data: std::ptr::null_mut() };
    let ok = unsafe {
        CryptUnprotectData(&input, std::ptr::null_mut(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), UI_FORBIDDEN, &mut output)
    };
    if ok == 0 {
        return None;
    }
    let out = unsafe { std::slice::from_raw_parts(output.data, output.len as usize).to_vec() };
    unsafe { LocalFree(output.data as *mut _) };
    Some(out)
}

#[cfg(not(windows))]
fn protect(data: &[u8]) -> anyhow::Result<Vec<u8>> {
    Ok(data.to_vec())
}

#[cfg(not(windows))]
fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    Some(data.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_encrypted() {
        let dir = std::env::temp_dir().join(format!("flowbit-keys-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = KeyStore::new(&dir);
        store.set("deepl", "abc123:fx").unwrap();
        assert_eq!(store.get("deepl").as_deref(), Some("abc123:fx"));
        assert!(!std::fs::read_to_string(dir.join("keys.json")).unwrap().contains("abc123"));
        assert_eq!(store.saved(), vec!["deepl".to_string()]);
        store.set("deepl", "").unwrap();
        assert_eq!(store.get("deepl"), None);
        std::fs::remove_dir_all(dir).ok();
    }
}
