use std::ptr;

use llmcc_core::symbol::{Symbol, SymbolKind};
use llmcc_descriptor::{CallChain, CallChainRoot, CallKind, CallSegment, CallSymbol, CallTarget};

use crate::binder::BinderCore;

/// Resolves `CallTarget`s discovered during collection into concrete `Symbol`s.
///
/// The resolver understands both simple targets like `foo::bar` and chained
/// method-style invocations such as `receiver.method().again()`. Each branch is
/// documented to clarify how it updates the receiver context and which fallbacks
/// are attempted when we cannot resolve a segment precisely.
pub struct CallTargetResolver<'core, 'tcx, 'collection> {
    binder: &'core BinderCore<'tcx, 'collection>,
}

impl<'core, 'tcx, 'collection> CallTargetResolver<'core, 'tcx, 'collection> {
    pub fn new(binder: &'core BinderCore<'tcx, 'collection>) -> Self {
        Self { binder }
    }

    pub fn resolve(&self, target: &CallTarget, out: &mut Vec<&'tcx Symbol>) {
        match target {
            CallTarget::Symbol(call) => self.resolve_symbol_target(call, out),
            CallTarget::Chain(chain) => self.resolve_chain_target(chain, out),
            CallTarget::Dynamic { .. } => {
                // Dynamic targets (macros, raw strings, etc.) do not map to static symbols.
            }
        }
    }

    fn resolve_symbol_target(&self, call: &CallSymbol, out: &mut Vec<&'tcx Symbol>) {
        let mut parts = call.qualifiers.clone();
        parts.push(call.name.clone());
        match call.kind {
            // Treat `receiver.method()` targets. Example: `user.clone()`.
            CallKind::Method => {
                panic!("symbol target CallKind::Method");
            }
            // Struct/enum constructors, e.g. `Vec::new()` or `Color::Red`.
            CallKind::Constructor => {
                panic!("symbol target CallKind::Constructorn");
                // if let Some(sym) = self.binder.lookup_symbol_kind_priority(
                //     &parts,
                //     &[SymbolKind::Struct, SymbolKind::Enum],
                //     None,
                // ) {
                //     self.push_symbol_unique(out, sym);
                // }
            }
            // Macro invocations such as `println!` or `debug::log!`.
            CallKind::Macro => {
                if let Some(sym) = self
                    .binder
                    .lookup_symbol(&parts, Some(SymbolKind::Macro), None)
                    .or_else(|| self.binder.lookup_symbol_fqn(&parts, SymbolKind::Macro))
                {
                    self.push_symbol_unique(out, sym);
                }
            }
            // Regular free functions (default) and unknown classifications.
            // Example: `std::mem::drop(value)`.
            CallKind::Function => {
                if let Some(sym) = self
                    .binder
                    .lookup_symbol(&parts, Some(SymbolKind::Function), None)
                    .or_else(|| self.binder.lookup_symbol_fqn(&parts, SymbolKind::Function))
                {
                    self.push_symbol_unique(out, sym);
                }
                self.push_type_from_qualifiers(out, &call.qualifiers);
            }
            CallKind::Unknown => {
                panic!("symbol target CallKind::Unknown");
            }
        }
    }

    fn resolve_chain_target(&self, chain: &CallChain, out: &mut Vec<&'tcx Symbol>) {
        let mut receivers = self.seed_receivers_from_root(chain, out);

        for segment in &chain.parts {
            receivers = match segment.kind {
                CallKind::Constructor => self.handle_constructor_segment(segment, out),
                CallKind::Function => self.handle_function_segment(segment, out),
                CallKind::Macro => self.handle_macro_segment(segment, out),
                CallKind::Method => self.handle_method_segment(segment, out, receivers),
                CallKind::Unknown => Vec::new(),
            };

            if receivers.is_empty() {
                break;
            }
        }
    }

    fn seed_receivers_from_root(
        &self,
        chain: &CallChain,
        out: &mut Vec<&'tcx Symbol>,
    ) -> Vec<&'tcx Symbol> {
        match &chain.root {
            CallChainRoot::Expr(expr) => {
                // - `value.iter().map(...)`: root is `CallChainRoot::Expr("value")`, so we try resolve
                //   `value` as a local variable and use its type as the starting receiver.
                self.resolve_chain_root_expr(expr, out)
            }
            CallChainRoot::Invocation(invocation) => {
                // - `Builder::new().step()`: root is an invocation, so we resolve `Builder::new`
                //   first, append it to `out`, then treat the constructor's return symbol as
                //   the receiver for the next segment (`step`).
                let start_len = out.len();
                self.resolve(invocation.target.as_ref(), out);
                out.iter()
                    .skip(start_len)
                    .last()
                    .map(|&symbol| self.receiver_types_for_symbol(symbol))
                    .unwrap_or_default()
            }
        }
    }

    fn resolve_chain_root_expr(
        &self,
        expr: &str,
        out: &mut Vec<&'tcx Symbol>,
    ) -> Vec<&'tcx Symbol> {
        let trimmed = expr.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        if let Some(local) =
            self.binder
                .lookup_symbol(&[trimmed.to_string()], Some(SymbolKind::Variable), None)
        {
            let type_of = local.type_of();
            let receivers = self.receiver_types_for_symbol(local);
            println!(
                "resolve_chain_root_expr resolved {} kind={:?} type_of={:?} receivers={}",
                trimmed,
                local.kind(),
                type_of,
                receivers.len()
            );
            return receivers;
        }

        if let Some(symbol) = self.lookup_simple_path(trimmed) {
            self.push_symbol_unique(out, symbol);
            return self.receiver_types_for_symbol(symbol);
        }

        Vec::new()
    }

    fn handle_constructor_segment(
        &self,
        segment: &CallSegment,
        out: &mut Vec<&'tcx Symbol>,
    ) -> Vec<&'tcx Symbol> {
        let mut receivers = Vec::new();

        if let Some(struct_sym) =
            self.binder
                .lookup_symbol(&[segment.name.clone()], Some(SymbolKind::Struct), None)
        {
            self.push_symbol_unique(out, struct_sym);
            receivers.push(struct_sym);
        }

        if let Some(enum_sym) =
            self.binder
                .lookup_symbol(&[segment.name.clone()], Some(SymbolKind::Enum), None)
        {
            self.push_symbol_unique(out, enum_sym);
            receivers.push(enum_sym);
        }

        receivers
    }

    fn handle_function_segment(
        &self,
        segment: &CallSegment,
        out: &mut Vec<&'tcx Symbol>,
    ) -> Vec<&'tcx Symbol> {
        if let Some(sym) = self
            .binder
            .lookup_symbol(&[segment.name.clone()], Some(SymbolKind::Function), None)
            .or_else(|| {
                self.binder
                    .lookup_symbol_fqn(&[segment.name.clone()], SymbolKind::Function)
            })
        {
            self.push_symbol_unique(out, sym);
            return self.symbol_type_symbol(sym).into_iter().collect();
        }

        Vec::new()
    }

    fn handle_macro_segment(
        &self,
        segment: &CallSegment,
        out: &mut Vec<&'tcx Symbol>,
    ) -> Vec<&'tcx Symbol> {
        if let Some(sym) = self
            .binder
            .lookup_symbol(&[segment.name.clone()], Some(SymbolKind::Macro), None)
            .or_else(|| {
                self.binder
                    .lookup_symbol_fqn(&[segment.name.clone()], SymbolKind::Macro)
            })
        {
            self.push_symbol_unique(out, sym);
        }

        Vec::new()
    }

    fn handle_method_segment(
        &self,
        segment: &CallSegment,
        out: &mut Vec<&'tcx Symbol>,
        receivers: Vec<&'tcx Symbol>,
    ) -> Vec<&'tcx Symbol> {
        println!(
            "handle {:?} receivers={}",
            segment.name,
            receivers.len()
        );
        let mut next_receivers = Vec::new();

        for receiver in &receivers {
            let mut parts = self.symbol_path_parts(receiver);
            parts.push(segment.name.clone());
            if let Some(sym) = self
                .binder
                .lookup_symbol(&parts, Some(SymbolKind::Function), None)
                .or_else(|| self.binder.lookup_symbol_fqn(&parts, SymbolKind::Function))
            {
                self.push_symbol_unique(out, sym);
                if let Some(ret) = self.symbol_type_symbol(sym) {
                    self.push_receiver_unique(&mut next_receivers, ret);
                } else {
                    self.push_receiver_unique(&mut next_receivers, receiver);
                }
            }
        }

        if !next_receivers.is_empty() {
            return next_receivers;
        }

        if let Some(sym) = self
            .binder
            .lookup_symbol(&[segment.name.clone()], Some(SymbolKind::Function), None)
            .or_else(|| {
                self.binder
                    .lookup_symbol_fqn(&[segment.name.clone()], SymbolKind::Function)
            })
        {
            self.push_symbol_unique(out, sym);
            return self.receiver_types_for_symbol(sym);
        }

        Vec::new()
    }

    fn push_symbol_unique(&self, out: &mut Vec<&'tcx Symbol>, symbol: &'tcx Symbol) {
        if out.iter().any(|existing| ptr::eq(*existing, symbol)) {
            return;
        }
        out.push(symbol);
    }

    fn push_receiver_unique(&self, list: &mut Vec<&'tcx Symbol>, symbol: &'tcx Symbol) {
        if list.iter().any(|existing| ptr::eq(*existing, symbol)) {
            return;
        }
        list.push(symbol);
    }

    fn push_type_from_qualifiers(&self, out: &mut Vec<&'tcx Symbol>, qualifiers: &[String]) {
        if qualifiers.is_empty() {
            return;
        }

        if let Some(sym) = self
            .binder
            .lookup_symbol(qualifiers, Some(SymbolKind::Struct), None)
        {
            self.push_symbol_unique(out, sym);
        }

        if let Some(sym) = self
            .binder
            .lookup_symbol(qualifiers, Some(SymbolKind::Enum), None)
        {
            self.push_symbol_unique(out, sym);
        }

        if let Some(sym) = self
            .binder
            .lookup_symbol(qualifiers, Some(SymbolKind::Trait), None)
        {
            self.push_symbol_unique(out, sym);
        }
    }

    fn symbol_type_symbol(&self, symbol: &'tcx Symbol) -> Option<&'tcx Symbol> {
        symbol
            .type_of()
            .and_then(|sym_id| self.binder.unit().opt_get_symbol(sym_id))
    }

    fn symbol_path_parts(&self, symbol: &'tcx Symbol) -> Vec<String> {
        let fqn = symbol.fqn_name.read().clone();
        let mut parts: Vec<String> = fqn
            .split("::")
            .filter(|part| !part.is_empty())
            .map(|part| part.to_string())
            .collect();
        if parts.is_empty() {
            parts.push(symbol.name.clone());
        }
        parts
    }

    fn receiver_types_for_symbol(&self, symbol: &'tcx Symbol) -> Vec<&'tcx Symbol> {
        match symbol.kind() {
            SymbolKind::Struct | SymbolKind::Enum | SymbolKind::Trait => vec![symbol],
            SymbolKind::Function => {
                if let Some(result) = self.symbol_type_symbol(symbol) {
                    return vec![result];
                }
                if let Some(receiver) = self.receiver_from_symbol_path(symbol) {
                    return vec![receiver];
                }
                Vec::new()
            }
            _ => self.symbol_type_symbol(symbol).into_iter().collect(),
        }
    }

    fn receiver_from_symbol_path(&self, symbol: &'tcx Symbol) -> Option<&'tcx Symbol> {
        let mut parts = self.symbol_path_parts(symbol);
        if parts.len() <= 1 {
            return None;
        }
        parts.pop();

        self.binder
            .lookup_symbol(&parts, Some(SymbolKind::Struct), None)
            .or_else(|| {
                self.binder
                    .lookup_symbol(&parts, Some(SymbolKind::Enum), None)
            })
            .or_else(|| {
                self.binder
                    .lookup_symbol(&parts, Some(SymbolKind::Trait), None)
            })
            .or_else(|| self.binder.lookup_symbol_fqn(&parts, SymbolKind::Struct))
            .or_else(|| self.binder.lookup_symbol_fqn(&parts, SymbolKind::Enum))
            .or_else(|| self.binder.lookup_symbol_fqn(&parts, SymbolKind::Trait))
    }

    fn lookup_simple_path(&self, expr: &str) -> Option<&'tcx Symbol> {
        if expr.is_empty()
            || expr.contains('(')
            || expr.contains(')')
            || expr.contains('.')
            || expr.contains(' ')
            || matches!(expr, "self" | "Self" | "super")
        {
            return None;
        }

        if !expr.contains("::") {
            return None;
        }

        let parts: Vec<String> = expr
            .split("::")
            .filter(|segment| !segment.is_empty())
            .map(|segment| segment.to_string())
            .collect();

        if parts.is_empty() {
            return None;
        }

        self.binder.lookup_symbol(&parts, None, None)
    }
}
