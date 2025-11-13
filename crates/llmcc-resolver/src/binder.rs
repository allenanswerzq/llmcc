//! Binder stub for symbol resolution.
//!
//! This module provides a placeholder for the binder functionality.
//! Full implementation pending integration with:
//! - llmcc_descriptor for type expressions and call targets
//! - Symbol lookup and resolution across scopes
//! - Type binding and relationship establishment

use llmcc_core::context::CompileUnit;
use llmcc_core::scope::{Scope, ScopeStack};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationDirection {
    Forward,
    Backward,
}

/// Binder for resolving symbols and managing symbol relationships.
///
/// This is a placeholder implementation pending full integration.
#[derive(Debug)]
pub struct BinderCore<'tcx> {
    unit: CompileUnit<'tcx>,
    scopes: ScopeStack<'tcx>,
    relation_direction: RelationDirection,
}

impl<'tcx> BinderCore<'tcx> {
    pub fn new(unit: CompileUnit<'tcx>, globals: &'tcx Scope<'tcx>) -> Self {
        let mut scopes = ScopeStack::new(&unit.cc.arena, &unit.cc.interner);
        scopes.push(globals);

        Self {
            unit,
            scopes,
            relation_direction: RelationDirection::Forward,
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
    pub fn scopes(&self) -> &ScopeStack<'tcx> {
        &self.scopes
    }

    #[inline]
    pub fn scopes_mut(&mut self) -> &mut ScopeStack<'tcx> {
        &mut self.scopes
    }

    pub fn set_forward_relation(&mut self) {
        self.relation_direction = RelationDirection::Forward;
    }

    pub fn set_backward_relation(&mut self) {
        self.relation_direction = RelationDirection::Backward;
    }
}
