#[derive(Debug, Clone)]
pub struct VariableDescriptor {
    pub name: String,
    pub type_hint: Option<String>,
    pub scope: VariableScope,
}

#[derive(Debug, Clone, Copy)]
pub enum VariableScope {
    Global,
    ClassLevel,
    FunctionLocal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableKind {
    Simple,
    Tuple,
    List,
}

impl VariableDescriptor {
    pub fn new(name: String, scope: VariableScope) -> Self {
        Self {
            name,
            type_hint: None,
            scope,
        }
    }

    pub fn with_type_hint(mut self, type_hint: String) -> Self {
        self.type_hint = Some(type_hint);
        self
    }
}
