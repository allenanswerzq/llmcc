use std::collections::VecDeque;

use llmcc_core::context::Context;
use llmcc_core::ir::{HirId, HirNode};
use tree_sitter::Node;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FnVisibility {
    Private,
    Public,
    Crate,
    Restricted(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionParameter {
    pub pattern: String,
    pub ty: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FunctionOwner {
    Free {
        modules: Vec<String>,
    },
    Impl {
        modules: Vec<String>,
        self_ty: String,
        trait_name: Option<String>,
    },
    Trait {
        modules: Vec<String>,
        trait_name: String,
    },
}

impl FunctionOwner {
    pub fn modules(&self) -> &[String] {
        match self {
            FunctionOwner::Free { modules }
            | FunctionOwner::Impl { modules, .. }
            | FunctionOwner::Trait { modules, .. } => modules,
        }
    }

    pub fn trait_name(&self) -> Option<&str> {
        match self {
            FunctionOwner::Impl { trait_name, .. } => trait_name.as_deref(),
            FunctionOwner::Trait { trait_name, .. } => Some(trait_name),
            FunctionOwner::Free { .. } => None,
        }
    }

    pub fn impl_type(&self) -> Option<&str> {
        match self {
            FunctionOwner::Impl { self_ty, .. } => Some(self_ty),
            _ => None,
        }
    }

    pub fn fqn(&self, fn_name: &str) -> String {
        let mut parts: Vec<String> = self.modules().to_vec();
        match self {
            FunctionOwner::Free { .. } => {}
            FunctionOwner::Impl { self_ty, .. } => parts.push(self_ty.clone()),
            FunctionOwner::Trait { trait_name, .. } => parts.push(trait_name.clone()),
        }
        parts.push(fn_name.to_string());
        parts.join("::")
    }
}

#[derive(Debug, Clone)]
pub struct FunctionDescriptor {
    pub hir_id: HirId,
    pub name: String,
    pub owner: FunctionOwner,
    pub visibility: FnVisibility,
    pub is_async: bool,
    pub is_const: bool,
    pub is_unsafe: bool,
    pub generics: Option<String>,
    pub where_clause: Option<String>,
    pub parameters: Vec<FunctionParameter>,
    pub return_type: Option<String>,
    pub signature: String,
    pub fqn: String,
}

impl FunctionDescriptor {
    pub fn from_hir<'tcx>(ctx: Context<'tcx>, node: &HirNode<'tcx>) -> Option<Self> {
        let ts_node = match node.inner_ts_node() {
            ts if ts.kind() == "function_item" => ts,
            _ => return None,
        };

        let name_node = ts_node.child_by_field_name("name")?;
        let name = clean(&node_text(ctx, name_node));
        let header_text = ctx
            .file()
            .get_text(ts_node.start_byte(), name_node.start_byte());
        let fn_index = header_text.rfind("fn").unwrap_or_else(|| header_text.len());
        let header_clean = clean(&header_text[..fn_index]);

        let owner = FunctionOwner::from_ts_node(ctx, ts_node);
        let visibility = FnVisibility::from_header(&header_clean);
        let is_async = header_clean
            .split_whitespace()
            .any(|token| token == "async");
        let is_const = header_clean
            .split_whitespace()
            .any(|token| token == "const");
        let is_unsafe = header_clean
            .split_whitespace()
            .any(|token| token == "unsafe");
        let generics = ts_node
            .child_by_field_name("type_parameters")
            .map(|n| clean(&node_text(ctx, n)));
        let where_clause = ts_node
            .child_by_field_name("where_clause")
            .map(|n| clean(&node_text(ctx, n)));
        let parameters = ts_node
            .child_by_field_name("parameters")
            .map(|n| parse_parameters(ctx, n))
            .unwrap_or_default();
        let return_type = ts_node
            .child_by_field_name("return_type")
            .map(|n| clean(&node_text(ctx, n)))
            .map(|text| text.trim_start_matches("->").trim().to_string())
            .filter(|s| !s.is_empty());

        let signature = {
            let body_start = ts_node
                .child_by_field_name("body")
                .map(|body| body.start_byte())
                .unwrap_or_else(|| ts_node.end_byte());
            clean(&ctx.file().get_text(ts_node.start_byte(), body_start))
        };

        let fqn = owner.fqn(&name);

        Some(FunctionDescriptor {
            hir_id: node.hir_id(),
            name,
            owner,
            visibility,
            is_async,
            is_const,
            is_unsafe,
            generics,
            where_clause,
            parameters,
            return_type,
            signature,
            fqn,
        })
    }

    pub fn set_fqn(&mut self, fqn: String) {
        self.fqn = fqn;
    }
}

impl FnVisibility {
    fn from_header(header: &str) -> Self {
        if let Some(index) = header.find("pub") {
            let rest = &header[index..];
            let compressed: String = rest.chars().filter(|c| !c.is_whitespace()).collect();
            if compressed.starts_with("pub(") && compressed.ends_with(')') {
                let inner = &compressed[4..compressed.len() - 1];
                if inner == "crate" {
                    FnVisibility::Crate
                } else {
                    FnVisibility::Restricted(inner.to_string())
                }
            } else {
                FnVisibility::Public
            }
        } else {
            FnVisibility::Private
        }
    }
}

impl FunctionOwner {
    fn from_ts_node<'tcx>(ctx: Context<'tcx>, mut node: Node<'tcx>) -> Self {
        let mut modules = VecDeque::new();
        let mut impl_info: Option<(String, Option<String>)> = None;
        let mut trait_name: Option<String> = None;

        while let Some(parent) = node.parent() {
            match parent.kind() {
                "mod_item" => {
                    if let Some(name_node) = parent.child_by_field_name("name") {
                        modules.push_front(clean(&node_text(ctx, name_node)));
                    }
                }
                "impl_item" => {
                    let ty = parent
                        .child_by_field_name("type")
                        .map(|n| clean(&node_text(ctx, n)))
                        .unwrap_or_else(|| "impl".to_string());
                    let trait_name_text = parent
                        .child_by_field_name("trait")
                        .map(|n| clean(&node_text(ctx, n)));
                    impl_info = Some((ty, trait_name_text));
                }
                "trait_item" => {
                    if trait_name.is_none() {
                        trait_name = parent
                            .child_by_field_name("name")
                            .map(|n| clean(&node_text(ctx, n)));
                    }
                }
                _ => {}
            }
            node = parent;
        }

        let modules: Vec<String> = modules.into_iter().collect();

        if let Some((self_ty, trait_impl)) = impl_info {
            FunctionOwner::Impl {
                modules,
                self_ty,
                trait_name: trait_impl,
            }
        } else if let Some(trait_name) = trait_name {
            FunctionOwner::Trait {
                modules,
                trait_name,
            }
        } else {
            FunctionOwner::Free { modules }
        }
    }
}

fn parse_parameters<'tcx>(ctx: Context<'tcx>, params_node: Node<'tcx>) -> Vec<FunctionParameter> {
    let mut params = Vec::new();
    let mut cursor = params_node.walk();
    for child in params_node.named_children(&mut cursor) {
        match child.kind() {
            "parameter" => {
                let pattern = child
                    .child_by_field_name("pattern")
                    .map(|n| clean(&node_text(ctx, n)))
                    .unwrap_or_else(|| clean(&node_text(ctx, child)));
                let ty = child
                    .child_by_field_name("type")
                    .map(|n| clean(&node_text(ctx, n)))
                    .filter(|s| !s.is_empty());
                params.push(FunctionParameter { pattern, ty });
            }
            "self_parameter" => {
                params.push(FunctionParameter {
                    pattern: clean(&node_text(ctx, child)),
                    ty: None,
                });
            }
            _ => {}
        }
    }
    params
}

fn node_text<'tcx>(ctx: Context<'tcx>, node: Node<'tcx>) -> String {
    ctx.file().get_text(node.start_byte(), node.end_byte())
}

fn clean(text: &str) -> String {
    let mut out = String::new();
    let mut last_was_ws = false;
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !last_was_ws && !out.is_empty() {
                out.push(' ');
            }
            last_was_ws = true;
        } else {
            out.push(ch);
            last_was_ws = false;
        }
    }
    out.trim().to_string()
}
