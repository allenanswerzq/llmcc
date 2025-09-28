fn top_level() {}

pub fn bar(x: i32) -> i32 { x + 1 }

async fn baz() -> Result<(), ()> { Ok(()) }

mod my_mod {
    fn in_module() {}
}

struct Foo;

impl Foo {
    fn method(&self) {}
    fn static_fn() -> i32 {
        42
    }
}

trait MyTrait {
    fn required(&self);
    fn provided() {
        println!("default");
    }
}

fn main() {
    top_level();
    let f = Foo;
    f.method();
    Foo::static_fn();
    my_mod::in_module();
}
