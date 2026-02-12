#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SecretEnforcementMode {
    #[default]
    Strict,
    Compat,
}

impl SecretEnforcementMode {
    pub fn from_str(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "strict" => Some(Self::Strict),
            "compat" | "compatible" => Some(Self::Compat),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Strict => "strict",
            Self::Compat => "compat",
        }
    }

    pub fn cycle(self, delta: i8) -> Self {
        if delta < 0 {
            match self {
                Self::Strict => Self::Compat,
                Self::Compat => Self::Strict,
            }
        } else {
            match self {
                Self::Strict => Self::Compat,
                Self::Compat => Self::Strict,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VaultSourceType {
    Prompt,
    File,
}

impl VaultSourceType {
    pub fn from_str(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "prompt" => Some(Self::Prompt),
            "file" => Some(Self::File),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Prompt => "prompt",
            Self::File => "file",
        }
    }

    pub fn cycle(current: Option<Self>, delta: i8) -> Option<Self> {
        if delta < 0 {
            match current {
                None => Some(Self::File),
                Some(Self::Prompt) => None,
                Some(Self::File) => Some(Self::Prompt),
            }
        } else {
            match current {
                None => Some(Self::Prompt),
                Some(Self::Prompt) => Some(Self::File),
                Some(Self::File) => None,
            }
        }
    }
}
