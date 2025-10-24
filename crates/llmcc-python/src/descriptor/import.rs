#[derive(Debug, Clone)]
pub struct ImportDescriptor {
    pub kind: ImportKind,
    pub module: String,
    pub names: Vec<String>,    // For "from X import Y, Z"
    pub alias: Option<String>, // For "import X as Y"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    Simple,   // import os
    From,     // from collections import defaultdict
    Relative, // from . import sibling
}

impl ImportDescriptor {
    pub fn new(module: String, kind: ImportKind) -> Self {
        Self {
            kind,
            module,
            names: Vec::new(),
            alias: None,
        }
    }

    pub fn add_name(&mut self, name: String) {
        self.names.push(name);
    }

    pub fn set_alias(&mut self, alias: String) {
        self.alias = Some(alias);
    }
}
