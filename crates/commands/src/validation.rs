use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    Info,
    Warning,
    Error,
    Fatal,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationWarning {
    pub code: String,
    pub message: String,
    pub severity: ValidationSeverity,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationResult {
    pub valid: bool,
    #[serde(default)]
    pub warnings: Vec<ValidationWarning>,
}

impl ValidationResult {
    pub fn valid() -> Self {
        Self {
            valid: true,
            warnings: Vec::new(),
        }
    }

    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            valid: false,
            warnings: vec![ValidationWarning {
                code: code.into(),
                message: message.into(),
                severity: ValidationSeverity::Error,
            }],
        }
    }

    pub fn merge(mut self, other: Self) -> Self {
        self.valid &= other.valid;
        self.warnings.extend(other.warnings);
        self
    }

    pub fn first_error_message(&self) -> Option<String> {
        self.warnings
            .iter()
            .find(|w| {
                matches!(
                    w.severity,
                    ValidationSeverity::Error | ValidationSeverity::Fatal
                )
            })
            .map(|w| w.message.clone())
    }
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self::valid()
    }
}
