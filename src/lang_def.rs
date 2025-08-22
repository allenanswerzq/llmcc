#[macro_export]
macro_rules! define_tokens {
    (
        $( ($const:ident, $id:expr, $str:expr, $kind:expr) ),* $(,)?
    ) => {
        /// Language context for HIR processing
        #[derive(Debug)]
        pub struct Language<'tcx> {
            pub ctx: &'tcx Context<'tcx>,
        }

        impl<'tcx> Language<'tcx> {
            /// Create a new Language instance
            pub fn new(ctx: &'tcx Context<'tcx>) -> Self {
                Self { ctx }
            }

            // Generate token ID constants
            $(
                pub const $const: u16 = $id;
            )*

            /// Get the HIR kind for a given token ID
            pub fn hir_kind(token_id: u16) -> HirKind {
                match token_id {
                    $(
                        Self::$const => $kind,
                    )*
                    _ => HirKind::Internal,
                }
            }

            /// Get the string representation of a token ID
            pub fn token_str(token_id: u16) -> Option<&'static str> {
                match token_id {
                    $(
                        Self::$const => Some($str),
                    )*
                    _ => None,
                }
            }

            /// Check if a token ID is valid
            pub fn is_valid_token(token_id: u16) -> bool {
                matches!(token_id, $(Self::$const)|*)
            }
        }

        /// Trait for visiting HIR nodes with type-specific dispatch
        trait HirVisitor<'tcx> {
            /// Visit a node, dispatching to the appropriate method based on token ID
            fn visit_node(&mut self, node: HirNode<'tcx>, lang: &Language<'tcx>) {
                match node.token_id() {
                    $(
                        Language::$const => paste::paste! { self.[<visit_ $const>](node, lang) },
                    )*
                    _ => self.visit_unknown(node, lang),
                }
            }

            /// Visit all children of a node
            fn visit_children(&mut self, node: &HirNode<'tcx>, lang: &Language<'tcx>) {
                for id in node.children() {
                    let child = lang.ctx.hir_node(*id);
                    self.visit_node(child, lang);
                }
            }

            /// Handle unknown/unrecognized token types
            fn visit_unknown(&mut self, node: HirNode<'tcx>, lang: &Language<'tcx>) {
                self.visit_children(&node, lang);
            }

            // Generate visit methods for each token type with visit_ prefix
            $(
                paste::paste! {
                    fn [<visit_ $const>](&mut self, node: HirNode<'tcx>, lang: &Language<'tcx>) {
                        self.visit_children(&node, lang)
                    }
                }
            )*
        }
    };
}
