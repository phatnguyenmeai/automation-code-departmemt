use serde::{Deserialize, Serialize};

/// The role of an agent within the virtual engineering department.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Project Manager - orchestrates the pipeline, aggregates final report.
    PM,
    /// Business Analyst - turns requirements into user stories + AC.
    BA,
    /// Backend developer - produces API contract / impl spec.
    Dev,
    /// Frontend developer - produces FE component spec.
    Frontend,
    /// QA engineer - designs test plan, executes integration + UI tests.
    Test,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::PM => "pm",
            Role::BA => "ba",
            Role::Dev => "dev",
            Role::Frontend => "frontend",
            Role::Test => "test",
        }
    }

    pub fn all() -> &'static [Role] {
        &[Role::PM, Role::BA, Role::Dev, Role::Frontend, Role::Test]
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
