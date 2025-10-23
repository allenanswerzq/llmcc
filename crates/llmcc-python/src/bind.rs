use llmcc_core::context::CompileUnit;
use llmcc_core::interner::InternedStr;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{Scope, ScopeStack, Symbol, SymbolKind};

use crate::token::LangPython;

#[derive(Debug, Default)]
pub struct BindingResult {
    pub calls: Vec<CallBinding>,
}

#[derive(Debug, Clone)]
pub struct CallBinding {
    pub caller: String,
    pub target: String,
}

#[derive(Debug)]
struct SymbolBinder<'tcx> {
    unit: CompileUnit<'tcx>,
    scopes: ScopeStack<'tcx>,
    calls: Vec<CallBinding>,
}

impl<'tcx> SymbolBinder<'tcx> {
    pub fn new(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>) -> Self {
        let mut scopes = ScopeStack::new(&unit.cc.arena, &unit.cc.interner, &unit.cc.symbol_map);
        scopes.push(globals);
        Self {
            unit,
            scopes,
            calls: Vec::new(),
        }
    }

    fn interner(&self) -> &llmcc_core::interner::InternPool {
        self.unit.interner()
    }

    fn current_symbol(&self) -> Option<&'tcx Symbol> {
        self.scopes.scoped_symbol()
    }

    #[allow(dead_code)]
    fn visit_children_scope(&mut self, node: &HirNode<'tcx>, symbol: Option<&'tcx Symbol>) {
        let depth = self.scopes.depth();
        if let Some(symbol) = symbol {
            if let Some(parent) = self.scopes.scoped_symbol() {
                parent.add_dependency(symbol);
            }
        }

        let scope = self.unit.opt_get_scope(node.hir_id());
        if let Some(scope) = scope {
            self.scopes.push_with_symbol(scope, symbol);
            self.visit_children(node);
            self.scopes.pop_until(depth);
        } else {
            self.visit_children(node);
        }
    }

    fn lookup_symbol_suffix(
        &mut self,
        suffix: &[InternedStr],
        kind: Option<SymbolKind>,
    ) -> Option<&'tcx Symbol> {
        let file_index = self.unit.index;
        self.scopes
            .find_scoped_suffix_with_filters(suffix, kind, Some(file_index))
            .or_else(|| {
                self.scopes
                    .find_scoped_suffix_with_filters(suffix, kind, None)
            })
            .or_else(|| {
                self.scopes
                    .find_global_suffix_with_filters(suffix, kind, Some(file_index))
            })
            .or_else(|| {
                self.scopes
                    .find_global_suffix_with_filters(suffix, kind, None)
            })
    }

    fn add_symbol_relation(&mut self, symbol: Option<&'tcx Symbol>) {
        if let (Some(current), Some(target)) = (self.current_symbol(), symbol) {
            current.add_dependency(target);
        }
    }

    fn visit_children(&mut self, node: &HirNode<'tcx>) {
        let mut cursor = node.inner_ts_node().walk();

        for child in node.inner_ts_node().children(&mut cursor) {
            if let Some(child_hir) = self.unit.opt_hir_node(llmcc_core::HirId(child.id() as u32)) {
                self.visit_node(&child_hir);
            }
        }
    }

    fn visit_node(&mut self, node: &HirNode<'tcx>) {
        let kind = node.kind();
        let ts_node = node.inner_ts_node();
        let kind_id = ts_node.kind_id();

        match kind {
            llmcc_core::ir::HirKind::Scope => {
                if kind_id == LangPython::function_definition
                    || kind_id == LangPython::class_definition
                {
                    // Enter scope for function/class
                    let scope = self.unit.opt_get_scope(node.hir_id());
                    if let Some(scope) = scope {
                        let depth = self.scopes.depth();
                        let symbol = self.current_symbol();
                        self.scopes.push_with_symbol(scope, symbol);
                        self.visit_children(node);
                        self.scopes.pop_until(depth);
                    } else {
                        self.visit_children(node);
                    }
                } else {
                    self.visit_children(node);
                }
            }
            llmcc_core::ir::HirKind::Internal => {
                if kind_id == LangPython::call {
                    self.visit_call(node);
                } else {
                    self.visit_children(node);
                }
            }
            _ => {
                self.visit_children(node);
            }
        }
    }

    fn visit_call(&mut self, node: &HirNode<'tcx>) {
        // Extract function being called
        let ts_node = node.inner_ts_node();

        // In tree-sitter-python, call has a `function` field
        if let Some(func_node) = ts_node.child_by_field_name("function") {
            if let Some(func_hir) = self
                .unit
                .opt_hir_node(llmcc_core::HirId(func_node.id() as u32))
            {
                if func_hir.kind() == llmcc_core::ir::HirKind::Identifier {
                    let content = self.unit.file().content();
                    if let Ok(name) = func_node.utf8_text(&content) {
                        let key = self.interner().intern(name);
                        if let Some(target) =
                            self.lookup_symbol_suffix(&[key], Some(SymbolKind::Function))
                        {
                            self.add_symbol_relation(Some(target));

                            // Track the call
                            let caller_name = self
                                .current_symbol()
                                .and_then(|s| Some(s.fqn_name.borrow().clone()))
                                .unwrap_or_else(|| "<module>".to_string());
                            let target_name = target.fqn_name.borrow().clone();

                            self.calls.push(CallBinding {
                                caller: caller_name,
                                target: target_name,
                            });
                        }
                    }
                }
            }
        }

        self.visit_children(node);
    }
}

pub fn bind_symbols<'tcx>(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>) -> BindingResult {
    let mut binder = SymbolBinder::new(unit, globals);

    if let Some(root) = unit.opt_hir_node(llmcc_core::HirId(0)) {
        binder.visit_children(&root);
    }

    BindingResult {
        calls: binder.calls,
    }
}
