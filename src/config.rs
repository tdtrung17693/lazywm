use std::{collections::HashMap, fs::File};

use serde::{Deserialize, Deserializer, Serialize};
use x11rb::protocol::xproto::KeyButMask;
use xkbcommon::xkb::{Keysym, KEYSYM_NO_FLAGS};

const CONFIG_FILE: &str = "config.ron";
const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");

#[derive(Debug, Deserialize, Serialize)]
pub enum ModKey {
    Mod1,
    Mod2,
    Mod3,
    Mod4,
    Mod5,
}

#[derive(Debug, Deserialize, Serialize, Hash, Eq, PartialEq)]
struct KeyCompound {
    modifier_mask: u32,
    keysym: Keysym,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    #[serde(deserialize_with = "deserialize_mod_key")]
    pub mod_key: u32,
    modes: HashMap<String, ConfigMode>,
    custom_commands: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ConfigMode {
    #[serde(deserialize_with = "deserialize_key_maps")]
    key_maps: Option<HashMap<Keysym, HashMap<u32, String>>>,
}

fn deserialize_mod_key<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let s: Option<ModKey> = Deserialize::deserialize(deserializer)?;
    Ok(match s {
        Some(ModKey::Mod1) => KeyButMask::MOD1,
        Some(ModKey::Mod2) => KeyButMask::MOD2,
        Some(ModKey::Mod3) => KeyButMask::MOD3,
        Some(ModKey::Mod4) => KeyButMask::MOD4,
        Some(ModKey::Mod5) => KeyButMask::MOD5,
        None => KeyButMask::MOD1,
    }
    .into())
}

fn deserialize_key_maps<'de, D>(
    deserializer: D,
) -> Result<Option<HashMap<Keysym, HashMap<u32, String>>>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: HashMap<String, String> = Deserialize::deserialize(deserializer)?;
    let mut key_maps = HashMap::new();
    for (key, handler_name) in s {
        let (mod_mask, key_sym): (u32, u32) = key.split('+').fold((0, 0), |(mod_mask, _), k| {
            let k = k.trim();
            let mut key_sym = 0;
            let mut current_mod_mask = mod_mask;

            if let Some(m) = get_modifier_mask(k) {
                current_mod_mask |= <KeyButMask as Into<u32>>::into(m)
            } else {
                key_sym = xkbcommon::xkb::keysym_from_name(k, KEYSYM_NO_FLAGS);
            }
            (current_mod_mask, key_sym)
        });
        let entry = key_maps.entry(key_sym).or_insert(HashMap::new());

        entry.insert(mod_mask, handler_name);
    }

    Ok(Some(key_maps))
}

fn get_modifier_mask(key: &str) -> Option<KeyButMask> {
    match key {
        "alt" | "Alt" => Some(KeyButMask::MOD1),
        "ctrl" | "Ctrl" => Some(KeyButMask::CONTROL),
        "shift" | "Shift" => Some(KeyButMask::SHIFT),
        "super" | "Super" => Some(KeyButMask::MOD4),
        _ => None,
    }
}

impl Config {
    pub fn get_key_maps(&self, mode: &str) -> Option<&HashMap<Keysym, HashMap<u32, String>>> {
        self.modes.get(mode).and_then(|m| m.key_maps.as_ref())
    }

    pub(crate) fn get_mod_mask(&self) -> u32 {
        return self.mod_key;
    }
}

pub fn load_config(path: Option<&str>) -> Result<Config, Box<dyn std::error::Error>> {
    let path = match path {
        Some(p) => p.to_string(),
        None => get_default_config_path(),
    };

    let f = File::open(&path).expect("Failed opening file");
    serde_yaml::from_reader(f).map_err(|e| e.into())
}

pub fn get_default_config_path() -> String {
    let xdg_config_home = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap();
        format!("{}/.config", home)
    });

    format!("{}/{}/{}", xdg_config_home, PACKAGE_NAME, CONFIG_FILE)
}
