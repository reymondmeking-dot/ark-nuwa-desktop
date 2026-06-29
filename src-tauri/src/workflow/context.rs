//! Execution context: global vars + accumulated node outputs, plus `{{...}}`
//! template interpolation.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub struct Context {
    /// Global workflow variables (e.g. `person`).
    vars: BTreeMap<String, String>,
    /// Outputs produced by nodes, keyed by their `output` key.
    outputs: BTreeMap<String, String>,
}

impl Context {
    pub fn new(vars: BTreeMap<String, String>) -> Self {
        Self { vars, outputs: BTreeMap::new() }
    }

    pub fn set_output(&mut self, key: &str, value: String) {
        self.outputs.insert(key.to_string(), value);
    }

    pub fn output(&self, key: &str) -> Option<&str> {
        self.outputs.get(key).map(|s| s.as_str())
    }

    pub fn outputs(&self) -> &BTreeMap<String, String> {
        &self.outputs
    }

    /// Look up a key first in node outputs, then in global vars.
    fn lookup(&self, key: &str) -> Option<&str> {
        self.outputs
            .get(key)
            .or_else(|| self.vars.get(key))
            .map(|s| s.as_str())
    }

    /// Replace every `{{ key }}` occurrence. Unknown keys render as an empty
    /// string but are reported, so callers can surface a clear error rather
    /// than silently sending a broken prompt.
    pub fn render(&self, template: &str) -> (String, Vec<String>) {
        let mut out = String::with_capacity(template.len());
        let mut missing = Vec::new();
        let mut rest = template;

        while let Some(start) = rest.find("{{") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            if let Some(end) = after.find("}}") {
                let raw_key = after[..end].trim();
                let (optional, key) = raw_key
                    .strip_prefix('?')
                    .map(|k| (true, k.trim()))
                    .unwrap_or((false, raw_key));
                match self.lookup(key) {
                    Some(v) => out.push_str(v),
                    None if optional => {}
                    None => missing.push(key.to_string()),
                }
                rest = &after[end + 2..];
            } else {
                // Unterminated `{{` — emit literally and stop.
                out.push_str("{{");
                rest = after;
            }
        }
        out.push_str(rest);
        (out, missing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Context {
        let mut vars = BTreeMap::new();
        vars.insert("person".to_string(), "段永平".to_string());
        let mut c = Context::new(vars);
        c.set_output("writings", "本分、平常心".to_string());
        c
    }

    #[test]
    fn renders_vars_and_outputs() {
        let (s, missing) = ctx().render("研究 {{person}} 的著作：{{writings}}");
        assert_eq!(s, "研究 段永平 的著作：本分、平常心");
        assert!(missing.is_empty());
    }

    #[test]
    fn reports_missing_keys() {
        let (s, missing) = ctx().render("{{unknown}} 结束");
        assert_eq!(s, " 结束");
        assert_eq!(missing, vec!["unknown".to_string()]);
    }

    #[test]
    fn handles_whitespace_in_braces() {
        let (s, _) = ctx().render("{{ person }}");
        assert_eq!(s, "段永平");
    }
}
