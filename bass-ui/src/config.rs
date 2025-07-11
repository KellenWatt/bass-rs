use std::path::{Path};
use std::fs::File;
use std::io::{Read, Write};

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub last_db: Option<String>,
    pub ui: UIConfig,
}


#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct UIConfig {
    pub default_font_size: f32,
}

impl Default for UIConfig {
    fn default() -> UIConfig {
        UIConfig {
            default_font_size: 16.0,
        }
    }
}

impl Config {
    pub fn load<P: AsRef<Path>>(root: P) -> Config {
        let file = root.as_ref().join("preferences.toml");
        match File::open(file) {
            Ok(mut f) => {
                let mut text = String::new();
                let Ok(_) = f.read_to_string(&mut text) else {
                    println!("something went wrong in read");
                    return Config::default();
                };
                toml::from_str(&text).unwrap_or_else(|_| Config::default())
            }
            Err(_) => {
                println!("file couldn't open for some reason");
                Config::default()
            }
        }
    }

    pub fn save<P: AsRef<Path>>(&self, root: P) -> std::io::Result<()> {
        let file = root.as_ref().join("preferences.toml");
        let mut file = File::create(file)?;
        write!(file, "{}", toml::to_string(self).unwrap())
    }
}
