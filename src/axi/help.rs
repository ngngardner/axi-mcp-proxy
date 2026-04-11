use std::fmt::Write;

use crate::config::ToolConfig;

/// Generate help text from a tool configuration.
#[must_use]
pub fn help(cfg: &ToolConfig) -> String {
    let mut buf = String::new();

    // write! to String is infallible
    let _ = writeln!(buf, "{}", cfg.description);

    if let Some(ref detailed) = cfg.detailed_help {
        let _ = writeln!(buf, "\n{detailed}");
    }

    if !cfg.parameters.is_empty() {
        let _ = writeln!(buf, "\nParameters:");
        for p in &cfg.parameters {
            let required = if p.required { " (required)" } else { "" };
            let _ = writeln!(
                buf,
                "  {} ({}): {}{}",
                p.name, p.param_type, p.description, required
            );
        }
    }

    let visible: Vec<_> = cfg
        .output_fields
        .iter()
        .filter(|f| f.default_visible)
        .collect();
    if !visible.is_empty() {
        let _ = writeln!(buf, "\nOutput fields:");
        for f in &visible {
            let _ = writeln!(buf, "  {}: {}", f.name, f.description);
        }
    }

    let hidden: Vec<_> = cfg
        .output_fields
        .iter()
        .filter(|f| !f.default_visible)
        .collect();
    if !hidden.is_empty() {
        let _ = writeln!(buf, "\nHidden fields (use --full):");
        for f in &hidden {
            let _ = writeln!(buf, "  {}: {}", f.name, f.description);
        }
    }

    if !cfg.aggregates.is_empty() {
        let _ = writeln!(buf, "\nAggregates:");
        for a in &cfg.aggregates {
            let _ = writeln!(buf, "  {}: {}", a.label, a.value);
        }
    }

    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::*;

    fn test_tool() -> ToolConfig {
        ToolConfig {
            description: "Search for items".into(),
            detailed_help: Some("Searches across all indexed items.".into()),
            parameters: vec![ParamConfig {
                name: "query".into(),
                param_type: ParamType::String,
                description: "Search query".into(),
                required: true,
            }],
            steps: vec![],
            output_fields: vec![
                OutputFieldConfig {
                    name: "id".into(),
                    description: "Item ID".into(),
                    max_len: None,
                    prefix: Some("#".into()),
                    default_visible: true,
                },
                OutputFieldConfig {
                    name: "tags".into(),
                    description: "Item tags".into(),
                    max_len: None,
                    prefix: None,
                    default_visible: false,
                },
            ],
            aggregates: vec![AggregateConfig {
                label: "results".into(),
                value: "count($step.s1)".into(),
                parsed_value: Some(AggregateExpr::Count("s1".into())),
            }],
            next_steps: vec![],
            empty_message: "No results.".into(),
            max_items: 10,
        }
    }

    #[test]
    fn test_help_description() {
        let output = help(&test_tool());
        assert!(output.contains("Search for items"));
        assert!(output.contains("Searches across all indexed items."));
    }

    #[test]
    fn test_help_parameters() {
        let output = help(&test_tool());
        assert!(output.contains("query (string): Search query (required)"));
    }

    #[test]
    fn test_help_visible_output_fields() {
        let output = help(&test_tool());
        assert!(output.contains("Output fields:"));
        assert!(output.contains("id: Item ID"));
    }

    #[test]
    fn test_help_hidden_fields() {
        let output = help(&test_tool());
        assert!(output.contains("Hidden fields (use --full):"));
        assert!(output.contains("tags: Item tags"));
    }

    #[test]
    fn test_help_aggregates() {
        let output = help(&test_tool());
        assert!(output.contains("Aggregates:"));
        assert!(output.contains("results: count($step.s1)"));
    }
}
