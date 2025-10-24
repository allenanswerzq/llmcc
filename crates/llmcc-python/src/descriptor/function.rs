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

    /// Extract parameters from a parameters node by walking the AST tree
    /// This method should be called with the parameters HirNode to populate the parameters field
    pub fn extract_parameters_from_ast<'tcx>(
        &mut self,
        params_node: &llmcc_core::ir::HirNode<'tcx>,
        unit: llmcc_core::context::CompileUnit<'tcx>,
    ) {
        use llmcc_core::ir::HirIdent;

        // Iterate through direct children of parameters node
        for child_id in params_node.children() {
            let child = unit.hir_node(*child_id);

            // Extract parameter information from the node
            self.extract_single_parameter(&child, unit);
        }
    }

    /// Extract a single parameter from a child node
    fn extract_single_parameter<'tcx>(
        &mut self,
        node: &llmcc_core::ir::HirNode<'tcx>,
        unit: llmcc_core::context::CompileUnit<'tcx>,
    ) {
        use crate::token::LangPython;

        let kind_id = node.kind_id();

        // Skip commas and other separators
        if kind_id == LangPython::Text_COMMA {
            return;
        }

        // Handle identifier nodes (simple parameters)
        if kind_id == LangPython::identifier {
            if let Some(ident) = node.as_ident() {
                self.add_parameter(FunctionParameter::new(ident.name.clone()));
            }
            return;
        }

        // Handle typed_parameter and typed_default_parameter
        if kind_id == LangPython::typed_parameter || kind_id == LangPython::typed_default_parameter
        {
            self.extract_typed_parameter(node, unit);
            return;
        }

        // Handle other parameter types (starred, etc.) - extract from text fallback
        let param_text = unit.get_text(
            node.inner_ts_node().start_byte(),
            node.inner_ts_node().end_byte(),
        );

        if !param_text.is_empty() && param_text != "(" && param_text != ")" {
            self.extract_parameter_from_text(&param_text);
        }
    }

    /// Extract typed parameter by walking its children
    fn extract_typed_parameter<'tcx>(
        &mut self,
        node: &llmcc_core::ir::HirNode<'tcx>,
        unit: llmcc_core::context::CompileUnit<'tcx>,
    ) {
        use crate::token::LangPython;

        let mut param_name = String::new();
        let mut param_type = None;
        let mut default_value = None;

        for child_id in node.children() {
            let child = unit.hir_node(*child_id);
            let kind_id = child.kind_id();

            if kind_id == LangPython::identifier {
                // First identifier is the parameter name
                if let Some(ident) = child.as_ident() {
                    if param_name.is_empty() {
                        param_name = ident.name.clone();
                    }
                }
            } else if kind_id == LangPython::type_node {
                // Type annotation - extract the type text from the AST node
                let type_text = unit.get_text(
                    child.inner_ts_node().start_byte(),
                    child.inner_ts_node().end_byte(),
                );
                if !type_text.is_empty() {
                    param_type = Some(type_text);
                }
            } else if kind_id != LangPython::Text_COLON && kind_id != LangPython::Text_EQ {
                // Other children might contain default values
                let child_text = unit.get_text(
                    child.inner_ts_node().start_byte(),
                    child.inner_ts_node().end_byte(),
                );
                if !child_text.is_empty() && child_text != "=" && child_text != ":" {
                    default_value = Some(child_text);
                }
            }
        }

        if !param_name.is_empty() {
            let mut param = FunctionParameter::new(param_name);
            if let Some(type_hint) = param_type {
                param = param.with_type_hint(type_hint);
            }
            if let Some(default) = default_value {
                param = param.with_default(default);
            }
            self.add_parameter(param);
        }
    }

    /// Extract parameter information from text (fallback for complex cases like *args, **kwargs)
    fn extract_parameter_from_text(&mut self, param_text: &str) {
        if param_text.is_empty() {
            return;
        }

        // Handle *args and **kwargs
        let (param_str, _is_var_args) = if param_text.starts_with("**") {
            (param_text[2..].to_string(), true)
        } else if param_text.starts_with("*") {
            (param_text[1..].to_string(), true)
        } else {
            (param_text.to_string(), false)
        };

        // Split name and type hint (if any)
        let (param_str, param_type) = if let Some(colon_pos) = param_str.find(':') {
            let name = param_str[..colon_pos].trim();
            let type_part = param_str[colon_pos + 1..].trim();

            // Extract just the type, not including default value
            let type_text = if let Some(eq_pos) = type_part.find('=') {
                type_part[..eq_pos].trim()
            } else {
                type_part
            };

            (name.to_string(), Some(type_text.to_string()))
        } else {
            (param_str, None)
        };

        // Split name and default value
        let (param_name, default_value) = if let Some(eq_pos) = param_str.find('=') {
            let name = param_str[..eq_pos].trim();
            let default = param_str[eq_pos + 1..].trim();
            (name.to_string(), Some(default.to_string()))
        } else {
            (param_str, None)
        };

        if !param_name.is_empty() {
            let mut param = FunctionParameter::new(param_name);
            if let Some(type_hint) = param_type {
                param = param.with_type_hint(type_hint);
            }
            if let Some(default) = default_value {
                param = param.with_default(default);
            }
            self.add_parameter(param);
        }
    }

    /// Extract return type from function definition node by walking the AST
    pub fn extract_return_type_from_ast<'tcx>(
        &mut self,
        func_def_node: &llmcc_core::ir::HirNode<'tcx>,
        unit: llmcc_core::context::CompileUnit<'tcx>,
    ) {
        let ts_node = func_def_node.inner_ts_node();
        let mut cursor = ts_node.walk();
        let mut found_arrow = false;

        for child in ts_node.children(&mut cursor) {
            if found_arrow && child.kind() == "type" {
                let return_text = unit.get_text(child.start_byte(), child.end_byte());
                if !return_text.is_empty() {
                    self.return_type = Some(return_text);
                }
                break;
            }
            if child.kind() == "->" {
                found_arrow = true;
            }
        }
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
