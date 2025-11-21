use crate::token::LangRust;

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum BinaryOperatorOutcome {
    ReturnsBool,
    ReturnsLeftOperand,
}

pub const BINARY_OPERATOR_TOKENS: &[(u16, BinaryOperatorOutcome)] = &[
    // "=="
    (LangRust::Text_EQEQ, BinaryOperatorOutcome::ReturnsBool),
    // "!="
    (LangRust::Text_NE, BinaryOperatorOutcome::ReturnsBool),
    // "<"
    (LangRust::Text_LT, BinaryOperatorOutcome::ReturnsBool),
    // ">"
    (LangRust::Text_GT, BinaryOperatorOutcome::ReturnsBool),
    // "<="
    (LangRust::Text_LE, BinaryOperatorOutcome::ReturnsBool),
    // ">="
    (LangRust::Text_GE, BinaryOperatorOutcome::ReturnsBool),
    // "&&"
    (LangRust::Text_AMPAMP, BinaryOperatorOutcome::ReturnsBool),
    // "||"
    (LangRust::Text_PIPEPIPE, BinaryOperatorOutcome::ReturnsBool),
    // "+"
    (
        LangRust::Text_PLUS,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ),
    // "-"
    (
        LangRust::Text_MINUS,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ),
    // "*"
    (
        LangRust::Text_STAR,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ),
    // "/"
    (
        LangRust::Text_SLASH,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ),
    // "%"
    (
        LangRust::Text_PERCENT,
        BinaryOperatorOutcome::ReturnsLeftOperand,
    ),
];

pub fn is_identifier_kind(kind_id: u16) -> bool {
    matches!(
        kind_id,
        LangRust::identifier
            | LangRust::scoped_identifier
            | LangRust::field_identifier
            | LangRust::type_identifier
    )
}
