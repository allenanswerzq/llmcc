use llmcc_core::context::CompileUnit;
use llmcc_core::symbol::{Scope, ScopeStack, Symbol};

#[derive(Debug)]
pub struct BinderCore<'tcx, 'a, C> {
    unit: CompileUnit<'tcx>,
    scopes: ScopeStack<'tcx>,
    collection: &'a C,
}

impl<'tcx, 'a, C> BinderCore<'tcx, 'a, C> {
    pub fn new(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>, collection: &'a C) -> Self {
        let mut scopes = ScopeStack::new(&unit.cc.arena, &unit.cc.interner, &unit.cc.symbol_map);
        scopes.push(globals);

        Self {
            unit,
            scopes,
            collection,
        }
    }

    #[inline]
    pub fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    #[inline]
    pub fn interner(&self) -> &llmcc_core::interner::InternPool {
        self.unit.interner()
    }

    #[inline]
    pub fn collection(&self) -> &'a C {
        self.collection
    }

    #[inline]
    pub fn scopes(&self) -> &ScopeStack<'tcx> {
        &self.scopes
    }

    #[inline]
    pub fn scopes_mut(&mut self) -> &mut ScopeStack<'tcx> {
        &mut self.scopes
    }

    #[inline]
    pub fn current_symbol(&self) -> Option<&'tcx Symbol> {
        self.scopes.scoped_symbol()
    }
}
