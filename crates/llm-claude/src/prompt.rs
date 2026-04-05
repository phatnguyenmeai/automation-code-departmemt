/// Small helper to build user prompts with structured JSON context blocks.
pub struct PromptBuilder {
    buf: String,
}

impl PromptBuilder {
    pub fn new() -> Self {
        Self { buf: String::new() }
    }

    pub fn section(mut self, title: &str, body: &str) -> Self {
        if !self.buf.is_empty() {
            self.buf.push_str("\n\n");
        }
        self.buf.push_str("## ");
        self.buf.push_str(title);
        self.buf.push('\n');
        self.buf.push_str(body);
        self
    }

    pub fn json_section(self, title: &str, value: &serde_json::Value) -> Self {
        let body = serde_json::to_string_pretty(value).unwrap_or_else(|_| "{}".into());
        self.section(title, &format!("```json\n{}\n```", body))
    }

    pub fn build(self) -> String {
        self.buf
    }
}

impl Default for PromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}
