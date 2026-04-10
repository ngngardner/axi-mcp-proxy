use std::fmt::Write;

use crate::config::ToolConfig;

/// Generate help text from a tool configuration.
pub fn help(cfg: &ToolConfig) -> String {
    let mut buf = String::new();

    writeln!(buf, "{}", cfg.description).unwrap();

    if let Some(ref detailed) = cfg.detailed_help {
        writeln!(buf, "\n{detailed}").unwrap();
    }

    if !cfg.parameters.is_empty() {
        writeln!(buf, "\nParameters:").unwrap();
        for p in &cfg.parameters {
            let required = if p.required { " (required)" } else { "" };
            writeln!(
                buf,
                "  {} ({}): {}{}",
                p.name, p.param_type, p.description, required
            )
            .unwrap();
        }
    }

    let visible: Vec<_> = cfg
        .output_fields
        .iter()
        .filter(|f| f.default_visible)
        .collect();
    if !visible.is_empty() {
        writeln!(buf, "\nOutput fields:").unwrap();
        for f in &visible {
            writeln!(buf, "  {}: {}", f.name, f.description).unwrap();
        }
    }

    let hidden: Vec<_> = cfg
        .output_fields
        .iter()
        .filter(|f| !f.default_visible)
        .collect();
    if !hidden.is_empty() {
        writeln!(buf, "\nHidden fields (use --full):").unwrap();
        for f in &hidden {
            writeln!(buf, "  {}: {}", f.name, f.description).unwrap();
        }
    }

    if !cfg.aggregates.is_empty() {
        writeln!(buf, "\nAggregates:").unwrap();
        for a in &cfg.aggregates {
            writeln!(buf, "  {}: {}", a.label, a.value).unwrap();
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
                param_type: "string".into(),
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
