fn top_level() {}

pub fn bar(x: i32) -> i32 {
    x + 1
}

async fn baz() -> Result<(), ()> {
    Ok(())
}

// Function returning a function
pub(crate) fn make_adder(y: i32) -> impl Fn(i32) -> i32 {
    move |x| x + y
}

// Function taking another function as argument
fn apply_twice<F>(f: F, x: i32) -> i32
where
    F: Fn(i32) -> i32,
{
    f(f(x))
}

// Function with a complex argument and return type
fn option_result_map(
    input: Option<Result<i32, &'static str>>,
) -> Result<Option<i32>, &'static str> {
    match input {
        Some(Ok(v)) => Ok(Some(v * 2)),
        Some(Err(e)) => Err(e),
        None => Ok(None),
    }
}

// Multiple return values via tuple
fn split_point(s: &str) -> (&str, &str) {
    let mid = s.len() / 2;
    s.split_at(mid)
}


pub mod my_mod {
    pub fn in_module() {}

    pub mod nest {
        pub fn in_nest_module() {}
    }
}


struct Foo;

impl Foo {
    fn method(&self) {}

    fn static_fn() -> i32 {
        42
    }

    fn returns_fn() -> fn(i32) -> i32 {
        |x| x * 2
    }

    fn takes_fn<F: Fn(i32) -> i32>(&self, f: F, v: i32) -> i32 {
        f(v)
    }
}

