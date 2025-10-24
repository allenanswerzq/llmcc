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
        // Use HIR children instead of tree-sitter children
        for child_id in node.children() {
            let child = self.unit.hir_node(*child_id);
            self.visit_node(&child);
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
                    // Look up the symbol for this function/class
                    if let Some(name_node) =
                        node.opt_child_by_field(self.unit, LangPython::field_name)
                    {
                        if let Some(ident) = name_node.as_ident() {
                            let key = self.interner().intern(&ident.name);
                            let file_index = self.unit.index;
                            let symbol = self
                                .scopes
                                .find_scoped_suffix_with_filters(&[key], None, Some(file_index))
                                .or_else(|| {
                                    self.scopes
                                        .find_scoped_suffix_with_filters(&[key], None, None)
                                })
                                .or_else(|| {
                                    self.scopes.find_global_suffix_with_filters(
                                        &[key],
                                        None,
                                        Some(file_index),
                                    )
                                })
                                .or_else(|| {
                                    self.scopes
                                        .find_global_suffix_with_filters(&[key], None, None)
                                });

                            // Check parent symbol BEFORE pushing new scope
                            let parent_symbol = self.scopes.scoped_symbol();

                            let scope = self.unit.opt_get_scope(node.hir_id());
                            if let Some(scope) = scope {
                                let depth = self.scopes.depth();
                                self.scopes.push_with_symbol(scope, symbol);

                                // If this is a method (function inside a class), add class->method dependency
                                if kind_id == LangPython::function_definition {
                                    if let Some(method_sym) = symbol {
                                        // Check if the parent scope symbol is a class
                                        if let Some(class_sym) = parent_symbol {
                                            if class_sym.kind() == SymbolKind::Struct {
                                                // This is a method inside a class
                                                class_sym.add_dependency(method_sym);
                                            }
                                        }
                                    }
                                }

                                self.visit_children(node);
                                self.scopes.pop_until(depth);
                            } else {
                                self.visit_children(node);
                            }
                        } else {
                            self.visit_children(node);
                        }
                    } else {
                        self.visit_children(node);
                    }
                } else if kind_id == LangPython::decorated_definition {
                    // Handle decorated definitions
                    self.visit_decorated_def(node);
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
    fn visit_decorated_def(&mut self, node: &HirNode<'tcx>) {
        // Extract decorators and the decorated definition (function or class)
        // A decorated_definition contains decorators followed by the actual function/class definition

        let mut decorator_symbols = Vec::new();
        let mut definition_idx = None;

        for (idx, child_id) in node.children().iter().enumerate() {
            let child = self.unit.hir_node(*child_id);
            let kind_id = child.kind_id();

            if kind_id == LangPython::decorator {
                // Extract the decorator name
                let content = self.unit.file().content();
                let ts_node = child.inner_ts_node();
                // Decorator text includes @, so we skip it
                if let Ok(decorator_text) = ts_node.utf8_text(&content) {
                    let decorator_name = decorator_text.trim_start_matches('@').trim();
                    let key = self.interner().intern(decorator_name);
                    if let Some(decorator_symbol) =
                        self.lookup_symbol_suffix(&[key], Some(SymbolKind::Function))
                    {
                        decorator_symbols.push(decorator_symbol);
                    }
                }
            } else if kind_id == LangPython::function_definition
                || kind_id == LangPython::class_definition
            {
                definition_idx = Some(idx);
                break; // The definition is always after decorators
            }
        }

        // Now visit the definition and apply decorators inside its scope
        if let Some(idx) = definition_idx {
            let definition_id = node.children()[idx];
            let definition = self.unit.hir_node(definition_id);
            let kind_id = definition.kind_id();

            // Enter scope for function/class
            if let Some(name_node) =
                definition.opt_child_by_field(self.unit, LangPython::field_name)
            {
                if let Some(ident) = name_node.as_ident() {
                    let key = self.interner().intern(&ident.name);
                    let file_index = self.unit.index;
                    let symbol = self
                        .scopes
                        .find_scoped_suffix_with_filters(&[key], None, Some(file_index))
                        .or_else(|| {
                            self.scopes
                                .find_scoped_suffix_with_filters(&[key], None, None)
                        })
                        .or_else(|| {
                            self.scopes.find_global_suffix_with_filters(
                                &[key],
                                None,
                                Some(file_index),
                            )
                        })
                        .or_else(|| {
                            self.scopes
                                .find_global_suffix_with_filters(&[key], None, None)
                        });

                    let scope = self.unit.opt_get_scope(definition.hir_id());
                    if let Some(scope) = scope {
                        let depth = self.scopes.depth();
                        self.scopes.push_with_symbol(scope, symbol);

                        // Apply decorators while in the function scope
                        if let Some(decorated_symbol) = self.current_symbol() {
                            for decorator_symbol in &decorator_symbols {
                                decorated_symbol.add_dependency(decorator_symbol);
                            }
                        }

                        self.visit_children(&definition);
                        self.scopes.pop_until(depth);
                    }
                }
            }
        }
    }

    fn visit_call(&mut self, node: &HirNode<'tcx>) {
        // Extract function being called
        let ts_node = node.inner_ts_node();

        // In tree-sitter-python, call has a `function` field
        if let Some(func_node) = ts_node.child_by_field_name("function") {
            let content = self.unit.file().content();
            if let Ok(name) = func_node.utf8_text(&content) {
                let key = self.interner().intern(name);
                if let Some(target) = self.lookup_symbol_suffix(&[key], Some(SymbolKind::Function))
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

        self.visit_children(node);
    }
}

pub fn bind_symbols<'tcx>(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>) -> BindingResult {
    let mut binder = SymbolBinder::new(unit, globals);

    if let Some(file_start_id) = unit.file_start_hir_id() {
        if let Some(root) = unit.opt_hir_node(file_start_id) {
            binder.visit_children(&root);
        }
    }

    BindingResult {
        calls: binder.calls,
    }
}
