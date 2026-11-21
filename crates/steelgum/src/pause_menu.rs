//! Authored settings shared by Steelgum's menu preview and the game.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PauseMenuConfig {
    pub title: String,
    pub sensitivity_label: String,
    pub resume_label: String,
    pub sensitivity_default: u8,
    pub sensitivity_min: u8,
    pub sensitivity_max: u8,
    pub sensitivity_step: u8,
}

impl Default for PauseMenuConfig {
    fn default() -> Self {
        Self {
            title: "Paused".into(),
            sensitivity_label: "Mouse sensitivity".into(),
            resume_label: "Resume".into(),
            sensitivity_default: 50,
            sensitivity_min: 10,
            sensitivity_max: 150,
            sensitivity_step: 5,
        }
    }
}

impl PauseMenuConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        if [&self.title, &self.sensitivity_label, &self.resume_label]
            .into_iter()
            .any(|label| !valid_label(label))
        {
            return Err("Menu labels need 1 to 24 printable ASCII characters.");
        }
        if self.sensitivity_min == 0
            || self.sensitivity_min > self.sensitivity_default
            || self.sensitivity_default > self.sensitivity_max
        {
            return Err(
                "Sensitivity must be positive, with the default between minimum and maximum.",
            );
        }
        if self.sensitivity_step == 0
            || self.sensitivity_step > self.sensitivity_max - self.sensitivity_min
        {
            return Err("Sensitivity step must be positive and fit within the range.");
        }
        Ok(())
    }

    pub fn decrease(&self, value: u8) -> u8 {
        value
            .saturating_sub(self.sensitivity_step)
            .max(self.sensitivity_min)
    }

    pub fn increase(&self, value: u8) -> u8 {
        value
            .saturating_add(self.sensitivity_step)
            .min(self.sensitivity_max)
    }
}

pub(crate) fn valid_label(label: &str) -> bool {
    !label.trim().is_empty()
        && label.len() <= 24
        && label.bytes().all(|byte| matches!(byte, b' '..=b'~'))
}
