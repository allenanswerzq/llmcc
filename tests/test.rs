fn foo(a: u16, b: u16) -> u16 {
    let mut x = 0;
    x = a + b;
    x
}

fn main() {
    let a = 1;
    let b = 2;
    foo(a, b);
}
