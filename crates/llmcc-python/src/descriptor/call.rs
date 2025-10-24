#[derive(Debug, Clone)]
pub struct CallDescriptor {
    pub target: CallTarget,
    pub arguments: Vec<CallArgument>,
}

#[derive(Debug, Clone)]
pub enum CallTarget {
    Function(String),
    Method(String, String), // object, method
    Constructor(String),    // class name
}

#[derive(Debug, Clone)]
pub struct CallArgument {
    pub name: Option<String>,
    pub value: String,
}

impl CallDescriptor {
    pub fn new(target: CallTarget) -> Self {
        Self {
            target,
            arguments: Vec::new(),
        }
    }

    pub fn add_argument(&mut self, arg: CallArgument) {
        self.arguments.push(arg);
    }
}

impl CallArgument {
    pub fn new(value: String) -> Self {
        Self { name: None, value }
    }

    pub fn with_name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }
}
