mod common;

use common::{find_symbol_id, with_collected_unit};
use llmcc_core::symbol::SymKind;
use textwrap::dedent;

use crate::common::with_compiled_unit;

#[serial_test::serial]
#[test]
fn test_visit_struct_item() {
    let source = dedent(
        "
        struct Point {
            x: i32,
            y: i32,
        }
        ",
    );
    with_compiled_unit(&[&source], |cc| {});
}
