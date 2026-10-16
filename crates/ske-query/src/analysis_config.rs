//! Strict, effective analysis configuration (§6.7, §11.4).
use serde::{Deserialize, Serialize};
use ske_common::{Error, Result};
use std::path::Path;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Budget {
    pub max_wall_time_ms: u64,
    pub max_states: u64,
    pub max_instructions: u64,
    pub max_call_depth: u32,
    pub loop_unroll: u32,
    pub loop_mode: String,
    pub max_recursion_unfold: u32,
    pub max_solver_queries: u64,
    pub solver_rlimit: u64,
    pub solver_wall_timeout_ms: u64,
    pub solver_seed: u32,
    pub max_memory_mb: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_wall_time_ms: 60_000,
            max_states: 10_000,
            max_instructions: 1_000_000,
            max_call_depth: 8,
            loop_unroll: 3,
            loop_mode: "unroll".into(),
            max_recursion_unfold: 2,
            max_solver_queries: 5_000,
            solver_rlimit: 5_000_000,
            solver_wall_timeout_ms: 10_000,
            solver_seed: 0,
            max_memory_mb: 1_024,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Context {
    pub name: String,
    pub param_range: Vec<ParamRange>,
}

impl Default for Context {
    fn default() -> Self {
        Self {
            name: "default".into(),
            param_range: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ParamRange {
    pub param: String,
    pub min: i64,
    pub max: i64,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct BudgetFile {
    budget: Budget,
}

fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::new("E_USAGE", format!("{}: {e}", path.display())))?;
    toml::from_str(&text).map_err(|e| Error::new("E_USAGE", e.to_string()))
}

impl Budget {
    pub fn read(path: Option<&Path>) -> Result<Self> {
        let value = path
            .map(read::<BudgetFile>)
            .transpose()?
            .unwrap_or_default()
            .budget;
        if !matches!(value.loop_mode.as_str(), "unroll" | "intervals") {
            return Err(Error::new(
                "E_USAGE",
                "loop_mode must be unroll or intervals",
            ));
        }
        // Zero exploration budgets are meaningful and intentionally accepted.
        // Z3 interprets zero as unlimited, so disallow it for safety limits.
        if value.solver_rlimit == 0 || value.solver_wall_timeout_ms == 0 {
            return Err(Error::new(
                "E_USAGE",
                "solver safety limits must be positive",
            ));
        }
        value.canonical()?;
        Ok(value)
    }

    pub fn canonical(&self) -> Result<String> {
        ske_common::canonical_json(
            &serde_json::to_value(self).map_err(|e| Error::new("E_INTERNAL", e.to_string()))?,
        )
    }
}

impl Context {
    pub fn read(path: Option<&Path>) -> Result<Self> {
        let value = path.map(read::<Self>).transpose()?.unwrap_or_default();
        if value.param_range.iter().any(|r| r.min > r.max) {
            return Err(Error::new("E_USAGE", "param_range min must be <= max"));
        }
        value.canonical()?;
        Ok(value)
    }

    pub fn canonical(&self) -> Result<String> {
        ske_common::canonical_json(
            &serde_json::to_value(self).map_err(|e| Error::new("E_INTERNAL", e.to_string()))?,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_config_defaults_and_strict_keys() {
        let defaults: BudgetFile = toml::from_str("").unwrap();
        assert_eq!(defaults.budget, Budget::default());
        assert_eq!(
            serde_json::to_value(defaults.budget)
                .unwrap()
                .as_object()
                .unwrap()
                .len(),
            12
        );
        assert!(toml::from_str::<BudgetFile>("[budget]\nmax_states=2\nunknown=1").is_err());
        assert!(toml::from_str::<BudgetFile>("max_states=2").is_err());
        assert!(toml::from_str::<BudgetFile>("[budget]\nmax_states=-1").is_err());
        assert!(toml::from_str::<Context>("unexpected=true").is_err());
        let value: Context = toml::from_str("[[param_range]]\nparam='x'\nmin=-1\nmax=10").unwrap();
        assert_eq!(value.param_range[0].min, -1);
        assert_eq!(value.name, "default");
        assert_eq!(
            value.canonical().unwrap(),
            "{\"name\":\"default\",\"param_range\":[{\"max\":10,\"min\":-1,\"param\":\"x\"}]}"
        );
    }
}
