pub mod binder;
pub mod collector;

pub use binder::BinderScopes;
pub use collector::CollectorScopes;
pub use collector::collect_symbols_with;
pub use binder::bind_symbols_with;
