use clap::Parser;
use std::io::Read;

#[derive(Debug)]
pub struct Config {
    pub svn_status_timeout: u8,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            svn_status_timeout: 2,
        }
    }
}

impl Config {
    pub fn update_from_env_args(&mut self) {
        let args = ConfigSource::parse();
        self.update(args);
    }

    pub fn update_from_file(&mut self) -> Result<(), String> {
        if let Ok(mut file) = std::fs::File::open("settings.toml") {
            let mut buf = String::new();
            file.read_to_string(&mut buf).map_err(|e| e.to_string())?;
            let parsed: ConfigSource = toml::from_str(&buf).map_err(|e| e.to_string())?;
            self.update(parsed);
        }
        Ok(())
    }

    fn update(&mut self, args: ConfigSource) {
        if let Some(n) = args.svn_timeout {
            self.svn_status_timeout = n;
        }
    }
}

#[derive(Parser, serde::Deserialize)]
#[command(version, about, long_about = None)]
struct ConfigSource {
    #[arg(short, long)]
    svn_timeout: Option<u8>,
}
