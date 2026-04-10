use anyhow::{Result, bail};
use std::collections::HashSet;

use crate::config::StepConfig;

/// Sort steps into parallel layers using Kahn's algorithm.
/// Steps in the same layer have all dependencies satisfied and can run concurrently.
///
/// # Errors
///
/// Returns an error if a dependency cycle is detected.
pub fn build_layers(steps: &[StepConfig]) -> Result<Vec<Vec<&StepConfig>>> {
    let mut placed: HashSet<&str> = HashSet::new();
    let mut layers: Vec<Vec<&StepConfig>> = Vec::new();

    while placed.len() < steps.len() {
        let mut layer: Vec<&StepConfig> = Vec::new();
        for step in steps {
            if placed.contains(step.name.as_str()) {
                continue;
            }
            let ready = step
                .depends_on
                .iter()
                .all(|dep| placed.contains(dep.as_str()));
            if ready {
                layer.push(step);
            }
        }
        if layer.is_empty() {
            bail!("cycle detected in step dependencies");
        }
        for step in &layer {
            placed.insert(&step.name);
        }
        layers.push(layer);
    }

    Ok(layers)
}

#[cfg(test)]
// Tests use unwrap/to_string/Default::default for brevity — panics are the desired failure mode
#[allow(
    clippy::unwrap_used,
    clippy::str_to_string,
    clippy::default_trait_access
)]
mod tests {
    use super::*;

    fn step(name: &str, deps: &[&str]) -> StepConfig {
        StepConfig {
            name: name.to_string(),
            upstream: "svc".to_string(),
            tool: "x".to_string(),
            args: Default::default(),
            depends_on: deps.iter().map(ToString::to_string).collect(),
            transform: None,
        }
    }

    #[test]
    fn test_no_dependencies() {
        let steps = vec![step("a", &[]), step("b", &[]), step("c", &[])];
        let layers = build_layers(&steps).unwrap();
        assert_eq!(layers.len(), 1);
        assert_eq!(layers[0].len(), 3);
    }

    #[test]
    fn test_linear_chain() {
        let steps = vec![step("a", &[]), step("b", &["a"]), step("c", &["b"])];
        let layers = build_layers(&steps).unwrap();
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0][0].name, "a");
        assert_eq!(layers[1][0].name, "b");
        assert_eq!(layers[2][0].name, "c");
    }

    #[test]
    fn test_parallel_with_shared_dependency() {
        let steps = vec![step("a", &[]), step("b", &["a"]), step("c", &["a"])];
        let layers = build_layers(&steps).unwrap();
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0].len(), 1);
        assert_eq!(layers[1].len(), 2);
    }

    #[test]
    fn test_cycle() {
        let steps = vec![step("a", &["b"]), step("b", &["a"])];
        let result = build_layers(&steps);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }
}
