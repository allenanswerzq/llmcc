#[derive(Debug, Clone)]
pub struct PythonClassDescriptor {
    pub name: String,
    pub base_classes: Vec<String>,
    pub methods: Vec<String>,
    pub fields: Vec<ClassField>,
}

#[derive(Debug, Clone)]
pub struct ClassField {
    pub name: String,
    pub type_hint: Option<String>,
}

impl PythonClassDescriptor {
    pub fn new(name: String) -> Self {
        Self {
            name,
            base_classes: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
        }
    }

    pub fn add_base_class(&mut self, base: String) {
        self.base_classes.push(base);
    }

    pub fn add_method(&mut self, method: String) {
        self.methods.push(method);
    }

    pub fn add_field(&mut self, field: ClassField) {
        self.fields.push(field);
    }
}

impl ClassField {
    pub fn new(name: String) -> Self {
        Self {
            name,
            type_hint: None,
        }
    }

    pub fn with_type_hint(mut self, type_hint: String) -> Self {
        self.type_hint = Some(type_hint);
        self
    }
}
