pub mod binder;
pub mod collector;

pub use binder::{BinderScopes, bind_symbols_with};
pub use collector::{CollectorScopes, collect_symbols_with};

#[derive(Default)]
pub struct ResolverOption {
    pub print_ir: bool,
    pub sequential: bool,
}

impl ResolverOption {
    pub fn with_print_ir(mut self, print_ir: bool) -> Self {
        self.print_ir = print_ir;
        self
    }

    pub fn with_sequential(mut self, sequential: bool) -> Self {
        self.sequential = sequential;
        self
    }
}
