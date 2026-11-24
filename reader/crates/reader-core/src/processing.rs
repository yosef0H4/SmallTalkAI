use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingMode {
    OneShot,
    #[default]
    Chunked,
}

impl fmt::Display for ProcessingMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OneShot => "one-shot",
            Self::Chunked => "chunked",
        })
    }
}

impl FromStr for ProcessingMode {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> anyhow::Result<Self> {
        match value {
            "one-shot" | "one_shot" => Ok(Self::OneShot),
            "chunked" => Ok(Self::Chunked),
            _ => anyhow::bail!("processing mode must be one-shot or chunked"),
        }
    }
}
