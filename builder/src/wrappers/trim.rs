use crate::manifest::TrimConfig;

use super::Wrapper;

pub struct Trim;

impl Wrapper for Trim {
    const NAME: &'static str = "trim";
    const NIX: &'static str = include_str!("trim.nix");
    type Config = TrimConfig;

    fn build_args(&self, cfg: &Self::Config) -> Vec<(&'static str, String)> {
        vec![
            ("strip", cfg.strip.to_string()),
            ("scrubToolchain", cfg.scrub_toolchain.to_string()),
        ]
    }
}
