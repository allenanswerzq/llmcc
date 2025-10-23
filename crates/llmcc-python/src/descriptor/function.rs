#[derive(Debug, Clone)]
pub struct PythonFunctionDescriptor {
    pub name: String,
    pub parameters: Vec<FunctionParameter>,
    pub return_type: Option<String>,
    pub decorators: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FunctionParameter {
    pub name: String,
    pub type_hint: Option<String>,
    pub default_value: Option<String>,
}

impl PythonFunctionDescriptor {
    pub fn new(name: String) -> Self {
        Self {
            name,
            parameters: Vec::new(),
            return_type: None,
            decorators: Vec::new(),
        }
    }

    pub fn add_parameter(&mut self, param: FunctionParameter) {
        self.parameters.push(param);
    }

    pub fn set_return_type(&mut self, return_type: String) {
        self.return_type = Some(return_type);
    }

    pub fn add_decorator(&mut self, decorator: String) {
        self.decorators.push(decorator);
    }
}

impl FunctionParameter {
    pub fn new(name: String) -> Self {
        Self {
            name,
            type_hint: None,
            default_value: None,
        }
    }

    pub fn with_type_hint(mut self, type_hint: String) -> Self {
        self.type_hint = Some(type_hint);
        self
    }

    pub fn with_default(mut self, default: String) -> Self {
        self.default_value = Some(default);
        self
    }
}
